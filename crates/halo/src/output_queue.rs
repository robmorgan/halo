// Adapted from timestretch-rs v0.16.0 desktop (MIT, Copyright 2026 Rob Morgan).
//! Desktop-owned SPSC device queue. It transports the library's epochs and
//! cumulative receipts alongside ratio-derived delivery timing (not sample origins).
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use timestretch::{AppliedParameters, Epoch, OutputConsumption, OutputStamp};

/// Transport coordinates at a software delivery boundary, never speaker time.
#[derive(Clone, Copy, Debug)]
pub struct Cursor {
    pub position: f64,
    pub iterations: u64,
    pub conversion: f64,
    pub region: Option<(usize, usize)>,
    pub total: f64,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            position: 0.0,
            iterations: 0,
            conversion: 1.0,
            region: None,
            total: f64::MAX,
        }
    }
}

impl Cursor {
    pub fn advance(&mut self, amount: f64) {
        self.position += amount * self.conversion;
        if let Some((start, end)) = self.region {
            let length = (end - start) as f64;
            if length > 0.0 && self.position >= end as f64 {
                let wraps = ((self.position - start as f64) / length).floor();
                self.iterations = self.iterations.saturating_add(wraps as u64);
                self.position = start as f64 + (self.position - start as f64).rem_euclid(length);
            }
        } else {
            self.position = self.position.min(self.total);
        }
    }
}

struct Queue {
    samples: Box<[AtomicU32]>,
    advances: Box<[AtomicU64]>,
    anchor: AtomicU64,
    conversion: AtomicU64,
    loop_start: AtomicU64,
    loop_end: AtomicU64,
    total: AtomicU64,
    anchor_revision: AtomicU64,
    capacity: u64,
    channels: usize,
    head: AtomicU64,
    tail: AtomicU64,
    generation: AtomicU64,
    epoch: AtomicU64,
    boundary: AtomicU64,
    receipt_generation: AtomicU64,
    receipt_epoch: AtomicU64,
    receipt_end: AtomicU64,
    ended: AtomicBool,
    underruns: AtomicU64,
}
pub struct Producer {
    queue: Arc<Queue>,
    epoch: Epoch,
    epoch_boundary: u64,
    next_frame: u64,
    applied: VecDeque<AppliedParameters>,
    advance: Option<f64>,
}
pub struct Consumer {
    queue: Arc<Queue>,
    epoch: Epoch,
    read_frames: u64,
    consumed_frames: u64,
    pending_advances: Box<[f64]>,
    pub cursor: Cursor,
    pub anchor_revision: u64,
    generation: u64,
}

pub fn channel(capacity_frames: usize) -> (Producer, Consumer) {
    channel_with_channels(capacity_frames, 2)
}
pub fn channel_with_channels(capacity_frames: usize, channels: usize) -> (Producer, Consumer) {
    assert!((1..=2).contains(&channels));
    assert!(capacity_frames >= 2048);
    let queue = Arc::new(Queue {
        samples: (0..capacity_frames * channels)
            .map(|_| AtomicU32::new(0))
            .collect(),
        advances: (0..capacity_frames)
            .map(|_| AtomicU64::new(1.0_f64.to_bits()))
            .collect(),
        anchor: AtomicU64::new(0.0_f64.to_bits()),
        conversion: AtomicU64::new(1.0_f64.to_bits()),
        loop_start: AtomicU64::new(0),
        loop_end: AtomicU64::new(0),
        total: AtomicU64::new(f64::MAX.to_bits()),
        anchor_revision: AtomicU64::new(0),
        channels,
        capacity: capacity_frames as u64,
        head: AtomicU64::new(0),
        tail: AtomicU64::new(0),
        generation: AtomicU64::new(0),
        epoch: AtomicU64::new(1),
        boundary: AtomicU64::new(0),
        receipt_generation: AtomicU64::new(0),
        receipt_epoch: AtomicU64::new(1),
        receipt_end: AtomicU64::new(0),
        ended: AtomicBool::new(false),
        underruns: AtomicU64::new(0),
    });
    (
        Producer {
            queue: queue.clone(),
            epoch: Epoch(1),
            epoch_boundary: 0,
            next_frame: 0,
            applied: VecDeque::new(),
            advance: None,
        },
        Consumer {
            queue,
            epoch: Epoch(1),
            read_frames: 0,
            consumed_frames: 0,
            pending_advances: vec![0.0; capacity_frames].into_boxed_slice(),
            cursor: Cursor::default(),
            anchor_revision: 0,
            generation: 0,
        },
    )
}
impl Producer {
    pub fn free_frames(&self) -> usize {
        // Stale frames still occupy storage until the consumer acknowledges
        // the O(1) boundary. They cannot be overwritten early.
        self.queue.capacity as usize
            - (self.queue.tail.load(Ordering::Acquire) - self.queue.head.load(Ordering::Acquire))
                as usize
    }
    pub fn occupied_frames(&self) -> usize {
        // Only the current epoch can satisfy the worker's priming reserve.
        (self.queue.tail.load(Ordering::Acquire)
            - self
                .queue
                .head
                .load(Ordering::Acquire)
                .max(self.epoch_boundary)) as usize
    }
    #[cfg(test)]
    pub fn publish_epoch(&mut self, epoch: Epoch) {
        self.publish_timing(epoch, Cursor::default(), 0);
    }

    pub fn publish_timing(&mut self, epoch: Epoch, cursor: Cursor, revision: u64) {
        self.queue.generation.fetch_add(1, Ordering::AcqRel);
        self.queue
            .anchor
            .store(cursor.position.to_bits(), Ordering::Relaxed);
        self.queue
            .conversion
            .store(cursor.conversion.to_bits(), Ordering::Relaxed);
        let (start, end) = cursor.region.unwrap_or((0, 0));
        self.queue.loop_start.store(start as u64, Ordering::Relaxed);
        self.queue.loop_end.store(end as u64, Ordering::Relaxed);
        self.queue
            .total
            .store(cursor.total.to_bits(), Ordering::Relaxed);
        self.queue
            .anchor_revision
            .store(revision, Ordering::Relaxed);
        self.epoch_boundary = self.queue.tail.load(Ordering::Relaxed);
        self.queue
            .boundary
            .store(self.epoch_boundary, Ordering::Relaxed);
        self.queue.epoch.store(epoch.0, Ordering::Relaxed);
        self.queue.ended.store(false, Ordering::Relaxed);
        self.queue.generation.fetch_add(1, Ordering::Release);
        self.epoch = epoch;
        self.next_frame = 0;
        self.applied.clear();
        self.advance = None;
    }
    pub fn retain_applied(&mut self, applied: AppliedParameters) {
        self.applied.push_back(applied);
    }

    pub fn initialize_advance(&mut self, applied: AppliedParameters) {
        self.advance.get_or_insert(applied.tempo());
    }

    pub fn push(&mut self, samples: &[f32], stamp: OutputStamp) -> bool {
        if stamp.epoch != self.epoch
            || stamp.start_frame != self.next_frame
            || samples.len() != stamp.frames * self.queue.channels
            || stamp.frames > self.free_frames()
        {
            return false;
        }
        let tail = self.queue.tail.load(Ordering::Relaxed);
        for (i, frame) in samples.chunks_exact(self.queue.channels).enumerate() {
            let output_frame = stamp.start_frame + i as u64;
            while self
                .applied
                .front()
                .is_some_and(|event| event.applied_at <= output_frame)
            {
                if let Some(event) = self.applied.pop_front() {
                    self.advance = Some(event.tempo());
                }
            }
            let slot = ((tail + i as u64) % self.queue.capacity) as usize;
            self.queue.advances[slot]
                .store(self.advance.unwrap_or(1.0).to_bits(), Ordering::Relaxed);
            let at = ((tail + i as u64) % self.queue.capacity) as usize * self.queue.channels;
            for (ch, &x) in frame.iter().enumerate() {
                self.queue.samples[at + ch].store(x.to_bits(), Ordering::Relaxed);
            }
        }
        self.next_frame += stamp.frames as u64;
        self.queue
            .tail
            .store(tail + stamp.frames as u64, Ordering::Release);
        true
    }
    pub fn receipt(&self) -> Option<OutputConsumption> {
        let a = self.queue.receipt_generation.load(Ordering::Acquire);
        if a & 1 != 0 {
            return None;
        }
        let epoch = Epoch(self.queue.receipt_epoch.load(Ordering::Relaxed));
        let end_frame = self.queue.receipt_end.load(Ordering::Relaxed);
        std::sync::atomic::fence(Ordering::Acquire);
        let b = self.queue.receipt_generation.load(Ordering::Relaxed);
        (a == b).then_some(OutputConsumption { epoch, end_frame })
    }
    pub fn finish(&self) {
        self.queue.ended.store(true, Ordering::Release);
    }
    pub fn underrun_frames(&self) -> u64 {
        self.queue.underruns.load(Ordering::Relaxed)
    }
}
impl Consumer {
    /// O(1) boundary handoff, including while paused. Never scans a stale backlog.
    pub fn sync_epoch(&mut self) -> bool {
        let a = self.queue.generation.load(Ordering::Acquire);
        if a & 1 != 0 {
            return false;
        }
        let epoch = Epoch(self.queue.epoch.load(Ordering::Relaxed));
        let boundary = self.queue.boundary.load(Ordering::Relaxed);
        let end = self.queue.loop_end.load(Ordering::Relaxed) as usize;
        let cursor = Cursor {
            position: f64::from_bits(self.queue.anchor.load(Ordering::Relaxed)),
            iterations: 0,
            conversion: f64::from_bits(self.queue.conversion.load(Ordering::Relaxed)),
            region: (end > 0)
                .then(|| (self.queue.loop_start.load(Ordering::Relaxed) as usize, end)),
            total: f64::from_bits(self.queue.total.load(Ordering::Relaxed)),
        };
        let revision = self.queue.anchor_revision.load(Ordering::Relaxed);
        std::sync::atomic::fence(Ordering::Acquire);
        let b = self.queue.generation.load(Ordering::Relaxed);
        if a != b || a == self.generation {
            return false;
        }
        self.queue.head.store(boundary, Ordering::Release);
        self.epoch = epoch;
        self.generation = a;
        self.cursor = cursor;
        self.anchor_revision = revision;
        self.read_frames = 0;
        self.consumed_frames = 0;
        self.publish_receipt();
        true
    }
    /// Inspect the first queued timing tag without consuming media.
    pub fn next_advance(&self) -> Option<f64> {
        let generation = self.queue.generation.load(Ordering::Acquire);
        if generation != self.generation || generation & 1 != 0 {
            return None;
        }
        let head = self.queue.head.load(Ordering::Relaxed);
        if head == self.queue.tail.load(Ordering::Acquire) {
            return None;
        }
        let value = f64::from_bits(
            self.queue.advances[(head % self.queue.capacity) as usize].load(Ordering::Relaxed),
        );
        std::sync::atomic::fence(Ordering::Acquire);
        (self.queue.generation.load(Ordering::Relaxed) == generation).then_some(value)
    }

    /// Copy available frames; the host acknowledges only what its output voice consumes.
    #[cfg(test)]
    pub fn read(&mut self, out: &mut [f32]) -> usize {
        self.read_timed(out, &mut [])
    }

    pub fn read_timed(&mut self, out: &mut [f32], advances: &mut [f64]) -> usize {
        advances.fill(0.0);
        let generation = self.queue.generation.load(Ordering::Acquire);
        if generation & 1 != 0 || self.queue.epoch.load(Ordering::Relaxed) != self.epoch.0 {
            out.fill(0.0);
            advances.fill(0.0);
            return 0;
        }
        let head = self.queue.head.load(Ordering::Relaxed);
        let tail = self.queue.tail.load(Ordering::Acquire);
        let n = (out.len() / self.queue.channels)
            .min((tail - head) as usize)
            .min(self.queue.capacity as usize - (self.read_frames - self.consumed_frames) as usize);
        for (i, frame) in out[..n * self.queue.channels]
            .chunks_exact_mut(self.queue.channels)
            .enumerate()
        {
            let advance = f64::from_bits(
                self.queue.advances[((head + i as u64) % self.queue.capacity) as usize]
                    .load(Ordering::Relaxed),
            );
            self.pending_advances[((self.read_frames + i as u64) % self.queue.capacity) as usize] =
                advance;
            if let Some(tag) = advances.get_mut(i) {
                *tag = advance;
            }
            let at = ((head + i as u64) % self.queue.capacity) as usize * self.queue.channels;
            for (ch, x) in frame.iter_mut().enumerate() {
                *x = f32::from_bits(self.queue.samples[at + ch].load(Ordering::Relaxed));
            }
        }
        out[n * self.queue.channels..].fill(0.0);
        // Publication may race the copy. Commit a read only if its complete
        // sample span belonged to the epoch observed before the copy. The
        // producer cannot reuse these slots until head advances below.
        std::sync::atomic::fence(Ordering::Acquire);
        if self.queue.generation.load(Ordering::Relaxed) != generation {
            out.fill(0.0);
            advances.fill(0.0);
            return 0;
        }
        self.queue.head.store(head + n as u64, Ordering::Release);
        self.read_frames += n as u64;
        if !self.queue.ended.load(Ordering::Acquire) {
            self.queue.underruns.fetch_add(
                (out.len() / self.queue.channels - n) as u64,
                Ordering::Relaxed,
            );
        }
        n
    }
    /// EOF is authoritative only after the library has finished publication
    /// and every delivered frame has a receipt. Round the final duration to
    /// the source endpoint so Play can rewind even at fractional ratios.
    pub fn complete(&mut self) -> bool {
        if self.cursor.region.is_none()
            && self.cursor.total != f64::MAX
            && self.queue.ended.load(Ordering::Acquire)
            && self.queue.generation.load(Ordering::Acquire) == self.generation
            && self.consumed_frames == self.read_frames
            && self.queue.head.load(Ordering::Acquire) == self.queue.tail.load(Ordering::Acquire)
            && self.cursor.position != self.cursor.total
        {
            self.cursor.position = self.cursor.total;
            return true;
        }
        false
    }

    pub fn acknowledge_frames(&mut self, count: usize) {
        let end = (self.consumed_frames + count as u64).min(self.read_frames);
        for frame in self.consumed_frames..end {
            self.cursor
                .advance(self.pending_advances[(frame % self.queue.capacity) as usize]);
        }
        self.consumed_frames = end;
        self.complete();
        self.publish_receipt();
    }
    fn publish_receipt(&self) {
        self.queue.receipt_generation.fetch_add(1, Ordering::AcqRel);
        self.queue
            .receipt_epoch
            .store(self.epoch.0, Ordering::Relaxed);
        self.queue
            .receipt_end
            .store(self.consumed_frames, Ordering::Relaxed);
        self.queue
            .receipt_generation
            .fetch_add(1, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(at: u64, rate: f64) -> AppliedParameters {
        AppliedParameters {
            requested: timestretch::Parameters::default(),
            achieved_stretch: 1.0 / rate,
            achieved_pitch: 1.0,
            command_id: at + 1,
            requested_at: 0,
            applied_at: at,
        }
    }

    #[test]
    fn mixed_applied_boundaries_partial_receipts_and_loop_conversion() {
        let (mut producer, mut consumer) = channel(2048);
        producer.publish_timing(
            Epoch(2),
            Cursor {
                position: 950.0,
                conversion: 2.0,
                region: Some((100, 1000)),
                ..Cursor::default()
            },
            7,
        );
        producer.initialize_advance(event(0, 0.5));
        producer.retain_applied(event(128, 1.0));
        assert!(producer.push(
            &[0.1; 512],
            OutputStamp {
                epoch: Epoch(2),
                start_frame: 0,
                frames: 256
            }
        ));
        consumer.sync_epoch();
        let mut audio = [0.0; 512];
        let mut tags = [0.0; 256];
        assert_eq!(consumer.read_timed(&mut audio, &mut tags), 256);
        assert_eq!(
            consumer.cursor.position, 950.0,
            "reads alone do not advance delivery"
        );
        assert_eq!(tags.iter().sum::<f64>(), 192.0);
        consumer.acknowledge_frames(128);
        assert_eq!(consumer.cursor.position, 178.0);
        consumer.acknowledge_frames(128);
        assert_eq!(consumer.cursor.position, 434.0);
        assert_eq!(consumer.cursor.iterations, 1);
        assert_eq!(consumer.anchor_revision, 7);
        assert_eq!(consumer.read_timed(&mut audio, &mut tags), 0);
        consumer.acknowledge_frames(256);
        assert_eq!(
            consumer.cursor.position, 434.0,
            "underrun silence is stationary"
        );
        producer.publish_timing(
            Epoch(3),
            Cursor {
                position: 123.75,
                ..Cursor::default()
            },
            8,
        );
        assert!(consumer.sync_epoch());
        assert_eq!(consumer.cursor.position, 123.75);
        assert_eq!(consumer.cursor.iterations, 0);
        assert_eq!(producer.receipt().unwrap().end_frame, 0);
    }

    #[test]
    fn timing_wrap_and_backpressure_keep_old_tags() {
        let (mut producer, mut consumer) = channel(2048);
        let mut expected = 0.0;
        let mut total = 0;
        for block in 0..50 {
            let rate = if block % 2 == 0 { 0.5 } else { 1.0 };
            producer.retain_applied(event(total, rate));
            assert!(producer.push(
                &[0.1; 4096],
                OutputStamp {
                    epoch: Epoch(1),
                    start_frame: total,
                    frames: 2048
                }
            ));
            assert!(!producer.push(
                &[0.2; 2],
                OutputStamp {
                    epoch: Epoch(1),
                    start_frame: total + 2048,
                    frames: 1
                }
            ));
            let mut read = 0;
            while read < 2048 {
                let n = consumer.read(&mut [0.0; 74]);
                consumer.acknowledge_frames(n);
                expected += n as f64 * rate;
                assert_eq!(consumer.cursor.position, expected);
                read += n;
            }
            total += 2048;
        }
    }

    #[test]
    fn stale_epoch_storage_never_counts_as_primed_media() {
        let (mut producer, mut consumer) = channel(2048);
        assert!(producer.push(
            &[1.0; 2048],
            OutputStamp {
                epoch: Epoch(1),
                start_frame: 0,
                frames: 1024,
            }
        ));
        producer.publish_epoch(Epoch(2));
        assert_eq!(producer.occupied_frames(), 0);
        assert_eq!(producer.free_frames(), 1024);
        assert!(producer.push(
            &[2.0; 1024],
            OutputStamp {
                epoch: Epoch(2),
                start_frame: 0,
                frames: 512,
            }
        ));
        assert_eq!(producer.occupied_frames(), 512);
        assert_eq!(producer.free_frames(), 512);
        assert!(consumer.sync_epoch());
        assert_eq!(producer.occupied_frames(), 512);
        assert_eq!(producer.free_frames(), 1536);
        let mut samples = [0.0; 1024];
        assert_eq!(consumer.read(&mut samples), 512);
        assert_eq!(samples, [2.0; 1024]);
    }
    #[test]
    fn concurrent_epoch_publication_never_mixes_or_duplicates_frames() {
        let (mut producer, mut consumer) = channel(2048);
        let done = Arc::new(AtomicBool::new(false));
        let end = done.clone();
        let writer = std::thread::spawn(move || {
            for epoch in 2..2002 {
                producer.publish_epoch(Epoch(epoch));
                for start in (0..512).step_by(32) {
                    let samples: Vec<_> = (start..start + 32)
                        .flat_map(|i| [epoch as f32, i as f32])
                        .collect();
                    let stamp = OutputStamp {
                        epoch: Epoch(epoch),
                        start_frame: start,
                        frames: 32,
                    };
                    while !producer.push(&samples, stamp) {
                        std::thread::yield_now();
                    }
                    std::thread::yield_now();
                }
            }
            end.store(true, Ordering::Release);
        });
        let mut samples = [0.0; 126];
        let mut expected = 0;
        while !done.load(Ordering::Acquire)
            || consumer.queue.head.load(Ordering::Acquire)
                < consumer.queue.tail.load(Ordering::Acquire)
        {
            if consumer.sync_epoch() {
                expected = 0;
            }
            let n = consumer.read(&mut samples);
            for frame in samples[..n * 2].chunks_exact(2) {
                assert_eq!(frame[0], consumer.epoch.0 as f32);
                assert_eq!(frame[1], expected as f32);
                expected += 1;
            }
            consumer.acknowledge_frames(n);
            std::thread::yield_now();
        }
        writer.join().unwrap();
    }
    #[test]
    fn partial_consumption_and_epoch_handoff() {
        let (mut producer, mut consumer) = channel(2048);
        assert!(producer.push(
            &[1.0; 200],
            OutputStamp {
                epoch: Epoch(1),
                start_frame: 0,
                frames: 100
            }
        ));
        let mut out = [0.0; 80];
        assert_eq!(consumer.read(&mut out), 40);
        consumer.acknowledge_frames(17);
        assert_eq!(producer.receipt().unwrap().end_frame, 17);
        producer.publish_epoch(Epoch(2));
        assert!(consumer.sync_epoch());
        assert_eq!(producer.occupied_frames(), 0);
        assert_eq!(
            producer.receipt().unwrap(),
            OutputConsumption {
                epoch: Epoch(2),
                end_frame: 0
            }
        );
        assert!(producer.push(
            &[2.0; 80],
            OutputStamp {
                epoch: Epoch(2),
                start_frame: 0,
                frames: 40
            }
        ));
        consumer.read(&mut out);
        assert_eq!(out, [2.0; 80]);
    }
    #[test]
    fn concurrent_wrap_preserves_samples_and_cumulative_receipts() {
        let (mut producer, mut consumer) = channel(2048);
        let writer = std::thread::spawn(move || {
            for block in 0..2000 {
                while producer.free_frames() < 37 {
                    std::thread::yield_now();
                }
                let samples = [block as f32; 74];
                assert!(producer.push(
                    &samples,
                    OutputStamp {
                        epoch: Epoch(1),
                        start_frame: block * 37,
                        frames: 37
                    }
                ));
            }
            producer
        });
        let mut frames = 0;
        let mut out = [0.0; 26];
        while frames < 74000 {
            let n = consumer.read(&mut out);
            for i in 0..n {
                assert_eq!(out[i * 2], ((frames + i) / 37) as f32);
            }
            frames += n;
            consumer.acknowledge_frames(n);
            if n == 0 {
                std::thread::yield_now();
            }
        }
        let producer = writer.join().unwrap();
        assert_eq!(producer.receipt().unwrap().end_frame, 74000);
    }
}
