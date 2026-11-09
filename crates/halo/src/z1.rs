//! The Traktor Kontrol Z1 over HID, the way Traktor and Mixxx drive it.
//! Its MIDI mode hides MODE and sends 7-bit values; HID has MODE, 12-bit
//! knobs, the positions at connect and a 7-step meter per channel.
//!
//! Its controls reach the profiles as [`MsgKind::Hid`] messages on their
//! own port, [`PORT`], so they bind and learn like any MIDI control. A
//! reader thread owns the device: it turns input reports into messages
//! and writes the LED report the UI hands it.

use crate::midi::{Feedback, MsgKey, MsgKind};

/// The Z1 as it's listed with the MIDI ports.
pub const PORT: &str = "Traktor Kontrol Z1 (HID)";

// Control numbers, for `MsgKey::hid`. The analog ones are in input report
// order.
pub const GAIN_A: u8 = 0;
pub const HI_A: u8 = 1;
pub const MID_A: u8 = 2;
pub const LOW_A: u8 = 3;
pub const FILTER_A: u8 = 4;
pub const GAIN_B: u8 = 5;
pub const HI_B: u8 = 6;
pub const MID_B: u8 = 7;
pub const LOW_B: u8 = 8;
pub const FILTER_B: u8 = 9;
pub const CUE_MIX: u8 = 10;
pub const VOLUME_A: u8 = 11;
pub const VOLUME_B: u8 = 12;
pub const CROSSFADER: u8 = 13;
pub const MODE: u8 = 14;
pub const FX_A: u8 = 15;
pub const FX_B: u8 = 16;
pub const HEADPHONE_A: u8 = 17;
pub const HEADPHONE_B: u8 = 18;
/// Output only.
pub const METER_A: u8 = 19;
pub const METER_B: u8 = 20;

const ANALOG: usize = 14;

const NAMES: [&str; 21] = [
    "GAIN A",
    "HI A",
    "MID A",
    "LOW A",
    "FILTER A",
    "GAIN B",
    "HI B",
    "MID B",
    "LOW B",
    "FILTER B",
    "CUE MIX",
    "VOLUME A",
    "VOLUME B",
    "CROSSFADER",
    "MODE",
    "FX ON A",
    "FX ON B",
    "HEADPHONE A",
    "HEADPHONE B",
    "METER A",
    "METER B",
];

pub fn name(number: u8) -> &'static str {
    NAMES.get(usize::from(number)).copied().unwrap_or("?")
}

pub const fn key(number: u8) -> MsgKey {
    MsgKey {
        kind: MsgKind::Hid,
        channel: 0,
        number,
    }
}

/// The buttons' bits in byte 0x1D of the input report.
const BUTTONS: [(u8, u8); 5] = [
    (MODE, 0x02),
    (FX_A, 0x04),
    (FX_B, 0x08),
    (HEADPHONE_A, 0x10),
    (HEADPHONE_B, 0x01),
];
const INPUT_ID: u8 = 0x01;
const INPUT_LEN: usize = 30;
const BUTTON_BYTE: usize = 0x1D;
/// The knobs and faders read 0..=4095.
const ANALOG_MAX: f32 = 4095.0;
/// Jitter: a knob at rest wanders by a count or two.
const DEADBAND: u16 = 3;

const OUTPUT_ID: u8 = 0x80;
pub const OUTPUT_LEN: usize = 22;
const METER_A_BYTES: usize = 0x01;
const METER_B_BYTES: usize = 0x08;
const HEADPHONE_A_LED: usize = 0x0F;
const HEADPHONE_B_LED: usize = 0x10;
const PLAY_A_LED: usize = 0x11;
const FX_A_LED: usize = 0x12;
const MODE_LED: usize = 0x13;
const PLAY_B_LED: usize = 0x14;
const FX_B_LED: usize = 0x15;
const LED_FULL: u8 = 0x7F;
/// A button that does something, at rest.
const LED_DIM: u8 = 0x0A;
/// Each meter LED's threshold, in dBFS of the deck's peak.
const METER_STEPS_DB: [f32; 7] = [-30.0, -20.0, -12.0, -8.0, -4.0, -2.0, -0.3];

/// The LED report for `feedback`. The FX buttons have two LEDs: blue for
/// their plain action, red for their shifted one while SHIFT is held. The
/// others show the layer in use, or the other layer when it has nothing.
pub fn report(feedback: &Feedback) -> [u8; OUTPUT_LEN] {
    let mut report = [0; OUTPUT_LEN];
    report[0] = OUTPUT_ID;
    let level = |on: Option<bool>| match on {
        Some(true) => LED_FULL,
        Some(false) => LED_DIM,
        None => 0,
    };
    for led in &feedback.leds {
        if led.key.kind != MsgKind::Hid {
            continue;
        }
        let layered = if feedback.shift {
            led.shifted.or(led.plain)
        } else {
            led.plain.or(led.shifted)
        };
        let red = |shifted: Option<bool>| {
            if feedback.shift && shifted == Some(true) {
                LED_FULL
            } else {
                0
            }
        };
        match led.key.number {
            FX_A => {
                report[FX_A_LED] = level(led.plain);
                report[PLAY_A_LED] = red(led.shifted);
            }
            FX_B => {
                report[FX_B_LED] = level(led.plain);
                report[PLAY_B_LED] = red(led.shifted);
            }
            HEADPHONE_A => report[HEADPHONE_A_LED] = level(layered),
            HEADPHONE_B => report[HEADPHONE_B_LED] = level(layered),
            MODE => report[MODE_LED] = level(layered),
            _ => {}
        }
    }
    for &(key, peak) in &feedback.meters {
        let first = match (key.kind, key.number) {
            (MsgKind::Hid, METER_A) => METER_A_BYTES,
            (MsgKind::Hid, METER_B) => METER_B_BYTES,
            _ => continue,
        };
        let db = 20.0 * peak.max(1e-6).log10();
        for (i, step) in METER_STEPS_DB.iter().enumerate() {
            report[first + i] = if db >= *step { LED_FULL } else { 0 };
        }
    }
    report
}

/// What changed between two input reports, as (control, value 0..1).
/// `last` holds each analog control's last sent count; `None` sends all.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn changes(
    report: &[u8],
    last: &mut Option<([u16; ANALOG], u8)>,
    crossfader: (u16, u16),
) -> Vec<(u8, f32, u16)> {
    let mut out = Vec::new();
    if report.len() < INPUT_LEN || report[0] != INPUT_ID {
        return out;
    }
    let counts: [u16; ANALOG] =
        std::array::from_fn(|i| u16::from_le_bytes([report[1 + 2 * i], report[2 + 2 * i]]));
    let buttons = report[BUTTON_BYTE];
    let (lo, hi) = crossfader;
    for (i, &count) in counts.iter().enumerate() {
        let moved = last.as_ref().is_none_or(|(prev, _)| {
            let prev = prev[i];
            // The ends always land, so a knob turned hard reads exactly 0 or 1.
            count.abs_diff(prev) >= DEADBAND || (count != prev && (count == 0 || count >= 4090))
        });
        if moved {
            let value = if i as u8 == CROSSFADER {
                (f32::from(count.saturating_sub(lo)) / f32::from(hi.saturating_sub(lo).max(1)))
                    .clamp(0.0, 1.0)
            } else {
                (f32::from(count) / ANALOG_MAX).clamp(0.0, 1.0)
            };
            out.push((i as u8, value, count));
        }
    }
    for (control, mask) in BUTTONS {
        let down = buttons & mask != 0;
        let was = last.as_ref().is_some_and(|(_, b)| b & mask != 0);
        // At connect only the held buttons count.
        if down != was {
            out.push((control, if down { 1.0 } else { 0.0 }, u16::from(down)));
        }
    }
    let mut counts_kept = last.map_or(counts, |(prev, _)| prev);
    for &(control, _, count) in &out {
        if usize::from(control) < ANALOG {
            counts_kept[usize::from(control)] = count;
        }
    }
    *last = Some((counts_kept, buttons));
    out
}

#[cfg(target_os = "macos")]
pub use hid::Z1;

#[cfg(target_os = "macos")]
mod hid {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, mpsc};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    use eframe::egui;
    use hidapi::{HidApi, HidDevice};

    use super::*;
    use crate::midi::MidiMsg;

    const VENDOR: u16 = 0x17CC;
    const PRODUCT: u16 = 0x1210;
    /// The crossfader's calibrated ends, in feature report 0xD2.
    const CALIBRATION_ID: u8 = 0xD2;
    const CROSSFADER_CAL: usize = 1 + 0x1C;
    /// Margin inside the calibrated ends, as Mixxx keeps.
    const CROSSFADER_MARGIN: u16 = 5;
    /// How long a read waits before the thread looks for LEDs to write.
    const READ_WAIT_MS: i32 = 4;
    /// How often a closed Z1 is looked for.
    const LOOK_EVERY: Duration = Duration::from_secs(1);
    const IDLE: Duration = Duration::from_millis(50);
    /// How long startup waits for the first look, so a Z1 that's plugged
    /// in is listed from the first frame.
    const FIRST_LOOK: Duration = Duration::from_millis(500);

    /// The Z1's own thread, which does all its HID work: macOS runs the
    /// calling thread's run loop while it enumerates devices, and on the
    /// UI thread that re-enters the window's event handler.
    ///
    /// It looks for the Z1, opens it while it's wanted, reads it into
    /// messages, and writes the LED report the UI last handed it.
    pub struct Z1 {
        reports: mpsc::Sender<[u8; OUTPUT_LEN]>,
        wanted: Arc<AtomicBool>,
        present: Arc<AtomicBool>,
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
        sent: Option<[u8; OUTPUT_LEN]>,
    }

    impl Z1 {
        pub fn spawn(events: mpsc::Sender<MidiMsg>, ctx: &egui::Context) -> Option<Self> {
            let (reports, rx) = mpsc::channel();
            let wanted = Arc::new(AtomicBool::new(false));
            let present = Arc::new(AtomicBool::new(false));
            let stop = Arc::new(AtomicBool::new(false));
            let flags = Flags {
                wanted: wanted.clone(),
                present: present.clone(),
                stop: stop.clone(),
            };
            let ctx = ctx.clone();
            let (looked_tx, looked) = mpsc::channel();
            let thread = std::thread::Builder::new()
                .name("halo-z1".into())
                .spawn(move || watch(&flags, &rx, &events, &ctx, &looked_tx))
                .map_err(|e| log::warn!("z1: thread: {e}"))
                .ok()?;
            let _ = looked.recv_timeout(FIRST_LOOK);
            Some(Self {
                reports,
                wanted,
                present,
                stop,
                thread: Some(thread),
                sent: None,
            })
        }

        /// Plugged in, as last looked.
        pub fn present(&self) -> bool {
            self.present.load(Ordering::Relaxed)
        }

        /// Open the Z1 while it's chosen; opening it ends its MIDI mode.
        pub fn set_wanted(&self, wanted: bool) {
            self.wanted.store(wanted, Ordering::Relaxed);
        }

        /// Light the LEDs for `feedback`, if that changes any.
        pub fn show(&mut self, feedback: &Feedback) {
            let report = report(feedback);
            if self.sent != Some(report) && self.reports.send(report).is_ok() {
                self.sent = Some(report);
            }
        }
    }

    impl Drop for Z1 {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    struct Flags {
        wanted: Arc<AtomicBool>,
        present: Arc<AtomicBool>,
        stop: Arc<AtomicBool>,
    }

    impl Flags {
        fn open(&self) -> bool {
            self.wanted.load(Ordering::Relaxed) && !self.stop.load(Ordering::Relaxed)
        }
    }

    fn watch(
        flags: &Flags,
        reports: &mpsc::Receiver<[u8; OUTPUT_LEN]>,
        events: &mpsc::Sender<MidiMsg>,
        ctx: &egui::Context,
        looked_tx: &mpsc::Sender<()>,
    ) {
        let mut api = match HidApi::new() {
            Ok(api) => api,
            Err(e) => {
                log::warn!("z1: hid: {e}");
                return;
            }
        };
        // The LEDs the UI wants, kept across a replug.
        let mut leds = None;
        let mut looked: Option<Instant> = None;
        // Said once until it opens: another app may hold the Z1 for good.
        let mut warned = false;
        while !flags.stop.load(Ordering::Relaxed) {
            while let Ok(report) = reports.try_recv() {
                leds = Some(report);
            }
            if looked.is_none_or(|t| t.elapsed() >= LOOK_EVERY) {
                looked = Some(Instant::now());
                let _ = api.reset_devices();
                let _ = api.add_devices(VENDOR, PRODUCT);
                let present = api.device_list().next().is_some();
                if flags.present.swap(present, Ordering::Relaxed) != present {
                    ctx.request_repaint();
                }
                let _ = looked_tx.send(());
            }
            if flags.open() && flags.present.load(Ordering::Relaxed) {
                match api.open(VENDOR, PRODUCT) {
                    Ok(device) => {
                        warned = false;
                        run(&device, flags, reports, &mut leds, events, ctx);
                        // Gone, or not wanted: either way look again now.
                        looked = None;
                    }
                    Err(e) => {
                        if !std::mem::replace(&mut warned, true) {
                            log::warn!("z1: open: {e}");
                        }
                        std::thread::sleep(LOOK_EVERY);
                    }
                }
            } else {
                std::thread::sleep(IDLE);
            }
        }
    }

    fn crossfader_ends(device: &HidDevice) -> (u16, u16) {
        let mut buf = [0u8; 64];
        buf[0] = CALIBRATION_ID;
        match device.get_feature_report(&mut buf) {
            Ok(n) if n >= CROSSFADER_CAL + 4 => {
                let lo = u16::from_le_bytes([buf[CROSSFADER_CAL], buf[CROSSFADER_CAL + 1]]);
                let hi = u16::from_le_bytes([buf[CROSSFADER_CAL + 2], buf[CROSSFADER_CAL + 3]]);
                if hi > lo + 2 * CROSSFADER_MARGIN {
                    return (lo + CROSSFADER_MARGIN, hi - CROSSFADER_MARGIN);
                }
                (0, ANALOG_MAX as u16)
            }
            _ => (0, ANALOG_MAX as u16),
        }
    }

    /// One open session, until the Z1 goes or isn't wanted.
    fn run(
        device: &HidDevice,
        flags: &Flags,
        reports: &mpsc::Receiver<[u8; OUTPUT_LEN]>,
        leds: &mut Option<[u8; OUTPUT_LEN]>,
        events: &mpsc::Sender<MidiMsg>,
        ctx: &egui::Context,
    ) {
        let crossfader = crossfader_ends(device);
        log::info!("z1: open, crossfader {}..{}", crossfader.0, crossfader.1);
        let port: Arc<str> = Arc::from(PORT);
        let mut last = None;
        let emit = |changes: Vec<(u8, f32, u16)>| {
            if changes.is_empty() {
                return;
            }
            for (control, value, raw) in changes {
                let _ = events.send(MidiMsg {
                    port: port.clone(),
                    key: key(control),
                    value,
                    raw,
                });
            }
            ctx.request_repaint();
        };
        // The Z1 only sends input reports once it's been sent an LED
        // report: what the UI asked for, or every button dim.
        let mut dim = [0; OUTPUT_LEN];
        dim[0] = OUTPUT_ID;
        for led in [
            HEADPHONE_A_LED,
            HEADPHONE_B_LED,
            FX_A_LED,
            FX_B_LED,
            MODE_LED,
        ] {
            dim[led] = LED_DIM;
        }
        let _ = device.write(&leds.unwrap_or(dim));
        // Where everything sits now, so Halo starts where the hardware is.
        let mut buf = [0u8; 64];
        buf[0] = INPUT_ID;
        if let Ok(n) = device.get_input_report(&mut buf) {
            emit(changes(&buf[..n], &mut last, crossfader));
        }
        while flags.open() {
            let mut latest = None;
            while let Ok(report) = reports.try_recv() {
                latest = Some(report);
            }
            if let Some(report) = latest {
                *leds = Some(report);
                if let Err(e) = device.write(&report) {
                    log::warn!("z1: write: {e}");
                    break;
                }
            }
            match device.read_timeout(&mut buf, READ_WAIT_MS) {
                Ok(0) => {}
                Ok(n) => emit(changes(&buf[..n], &mut last, crossfader)),
                Err(e) => {
                    log::warn!("z1: read: {e}");
                    return;
                }
            }
        }
        // Closing: everything off.
        let mut off = [0; OUTPUT_LEN];
        off[0] = OUTPUT_ID;
        let _ = device.write(&off);
        log::info!("z1: closed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::LedState;

    fn input(counts: [u16; ANALOG], buttons: u8) -> Vec<u8> {
        let mut report = vec![INPUT_ID];
        for c in counts {
            report.extend(c.to_le_bytes());
        }
        report.push(buttons);
        report
    }

    #[test]
    fn reads_positions_then_only_what_moved() {
        let mut last = None;
        let mut counts = [2048; ANALOG];
        counts[usize::from(CROSSFADER)] = 12;
        let first = changes(&input(counts, 0), &mut last, (12, 4083));
        assert_eq!(first.len(), ANALOG, "every position at connect, no buttons");
        assert_eq!(first[usize::from(CROSSFADER)].1, 0.0);

        // Jitter stays quiet; a real turn and a press come through.
        counts[usize::from(GAIN_A)] = 2049;
        assert!(changes(&input(counts, 0), &mut last, (12, 4083)).is_empty());
        counts[usize::from(GAIN_A)] = 4095;
        let moved = changes(&input(counts, 0x02 | 0x01), &mut last, (12, 4083));
        assert_eq!(
            moved,
            vec![(GAIN_A, 1.0, 4095), (MODE, 1.0, 1), (HEADPHONE_B, 1.0, 1)]
        );
        let released = changes(&input(counts, 0x01), &mut last, (12, 4083));
        assert_eq!(released, vec![(MODE, 0.0, 0)]);
    }

    #[test]
    fn leds_and_meters_land_in_the_report() {
        let feedback = Feedback {
            shift: true,
            leds: vec![
                LedState {
                    key: key(FX_A),
                    plain: Some(true),
                    shifted: Some(true),
                },
                LedState {
                    key: key(HEADPHONE_B),
                    plain: None,
                    shifted: Some(false),
                },
                LedState {
                    key: key(MODE),
                    plain: Some(true),
                    shifted: None,
                },
            ],
            meters: vec![
                (key(METER_A), 1.0),
                (key(METER_B), 10f32.powf(-10.0 / 20.0)),
            ],
        };
        let r = report(&feedback);
        assert_eq!(r[0], OUTPUT_ID);
        assert_eq!((r[FX_A_LED], r[PLAY_A_LED]), (LED_FULL, LED_FULL));
        assert_eq!(r[HEADPHONE_B_LED], LED_DIM);
        assert_eq!(r[HEADPHONE_A_LED], 0, "unbound stays dark");
        assert_eq!(r[MODE_LED], LED_FULL);
        assert!(
            r[METER_A_BYTES..METER_A_BYTES + 7]
                .iter()
                .all(|&b| b == LED_FULL)
        );
        // −10 dB: the first three steps.
        assert_eq!(
            r[METER_B_BYTES..METER_B_BYTES + 7],
            [LED_FULL, LED_FULL, LED_FULL, 0, 0, 0, 0]
        );
        // Without SHIFT the red play LED goes out.
        let r = report(&Feedback {
            shift: false,
            ..feedback
        });
        assert_eq!((r[FX_A_LED], r[PLAY_A_LED]), (LED_FULL, 0));
    }
}
