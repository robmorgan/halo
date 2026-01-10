//! Display renderer for Push 2.
//!
//! Renders DJ deck information and lighting status to the Push 2 display.

use super::frame_buffer::{FrameBuffer, DISPLAY_HEIGHT, DISPLAY_WIDTH};
use super::WaveformRenderer;
use crate::module::DeckDisplayState;

/// Width of each deck section (half the display)
const DECK_WIDTH: usize = DISPLAY_WIDTH / 2;

/// Colors (BGR565 format)
mod colors {
    pub const BLACK: u16 = 0x0000;
    pub const WHITE: u16 = 0xFFFF;
    pub const RED: u16 = 0xF800;
    pub const GREEN: u16 = 0x07E0;
    pub const BLUE: u16 = 0x001F;
    pub const ORANGE: u16 = 0xFD20;
    pub const CYAN: u16 = 0x07FF;
    pub const GRAY: u16 = 0x8410;
    pub const DARK_GRAY: u16 = 0x4208;
}

/// Display renderer for Push 2.
pub struct DisplayRenderer {
    waveform_a: WaveformRenderer,
    waveform_b: WaveformRenderer,
}

impl DisplayRenderer {
    /// Create a new display renderer.
    pub fn new() -> Self {
        Self {
            waveform_a: WaveformRenderer::new(),
            waveform_b: WaveformRenderer::new(),
        }
    }

    /// Render the full display with interpolated positions for smooth scrolling.
    pub fn render_with_positions(
        &mut self,
        buffer: &mut FrameBuffer,
        deck_a: &DeckDisplayState,
        deck_b: &DeckDisplayState,
        pos_a: f64,
        pos_b: f64,
    ) {
        buffer.clear();

        // Draw center divider
        buffer.draw_vline(DECK_WIDTH - 1, 0, DISPLAY_HEIGHT, colors::DARK_GRAY);
        buffer.draw_vline(DECK_WIDTH, 0, DISPLAY_HEIGHT, colors::DARK_GRAY);

        // Render each deck with interpolated position
        self.render_deck(buffer, deck_a, 0, pos_a);
        self.render_deck(buffer, deck_b, DECK_WIDTH + 2, pos_b);
    }

    /// Render a single deck section.
    fn render_deck(
        &mut self,
        buffer: &mut FrameBuffer,
        deck: &DeckDisplayState,
        x_offset: usize,
        position: f64,
    ) {
        // Waveform area (top 60 pixels)
        self.render_waveform(buffer, deck, x_offset, 0, DECK_WIDTH - 4, 60, position);

        // Track info (60-100)
        self.render_track_info(buffer, deck, x_offset, 62);

        // Transport state (100-130) - pass interpolated position
        self.render_transport(buffer, deck, x_offset, 102, position);

        // BPM (130-160)
        self.render_bpm(buffer, deck, x_offset, 132);
    }

    /// Render zoomed waveform centered on playhead (optimized).
    fn render_waveform(
        &self,
        buffer: &mut FrameBuffer,
        deck: &DeckDisplayState,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        position: f64,
    ) {
        // Draw waveform background
        buffer.draw_rect(x, y, w, h, colors::DARK_GRAY);

        // If no track loaded, show empty state
        if deck.title.is_empty() {
            buffer.draw_text(x + 10, y + h / 2 - 4, "No Track", colors::GRAY, 1);
            return;
        }

        let center_y = y + h / 2;
        let half_height = (h / 2) as f32;
        let waveform = &deck.waveform;

        // Check if we have waveform data
        if waveform.amplitudes.is_empty() || deck.duration_seconds <= 0.0 {
            buffer.draw_hline(x, center_y, w, colors::GRAY);
            buffer.draw_vline(x + w / 2, y, h, colors::WHITE);
            return;
        }

        // Visible time window (~8 seconds centered on playhead)
        let visible_seconds = 8.0;
        let start_time = (position - visible_seconds / 2.0).max(0.0);
        let end_time = (position + visible_seconds / 2.0).min(deck.duration_seconds);

        // Use pre-computed samples_per_second
        let start_sample = (start_time * waveform.samples_per_second) as usize;
        let end_sample =
            ((end_time * waveform.samples_per_second) as usize).min(waveform.amplitudes.len());
        let samples_in_view = end_sample.saturating_sub(start_sample);

        if samples_in_view > 0 {
            // Pre-calculate step for sample selection
            let step = samples_in_view as f32 / w as f32;

            for px in 0..w {
                let sample_idx = start_sample + (px as f32 * step) as usize;
                if sample_idx >= waveform.amplitudes.len() {
                    continue;
                }

                let amplitude = waveform.amplitudes[sample_idx];
                let bar_height = (amplitude * half_height * 0.9) as usize;

                if bar_height == 0 {
                    continue;
                }

                // Use pre-computed color
                let color = waveform.colors[sample_idx];

                // Draw mirrored waveform (optimized: draw line instead of pixel-by-pixel)
                let top_y = center_y.saturating_sub(bar_height);
                let bot_y = (center_y + bar_height).min(y + h - 1);
                buffer.draw_vline(x + px, top_y, bot_y - top_y, color);
            }
        }

        // Draw center line
        buffer.draw_hline(x, center_y, w, colors::GRAY);

        // Calculate playhead position using interpolated position
        let playhead_x = if position < visible_seconds / 2.0 {
            x + ((w / 2) as f64 * (position / (visible_seconds / 2.0))) as usize
        } else if position > deck.duration_seconds - visible_seconds / 2.0 {
            let time_from_end = deck.duration_seconds - position;
            x + w - ((w / 2) as f64 * (time_from_end / (visible_seconds / 2.0))) as usize
        } else {
            x + w / 2
        };

        // Draw playhead
        buffer.draw_vline(playhead_x, y, h, colors::WHITE);

        // Draw cue point if visible
        if let Some(cue) = deck.cue_point {
            if cue >= start_time && cue <= end_time {
                let cue_x =
                    x + (((cue - start_time) / (end_time - start_time)) * w as f64) as usize;
                buffer.draw_vline(cue_x, y, h, colors::ORANGE);
            }
        }

        // Draw hot cues if visible
        for (i, hot_cue) in deck.hot_cues.iter().enumerate() {
            if let Some(pos) = hot_cue {
                if *pos >= start_time && *pos <= end_time {
                    let hc_x =
                        x + (((*pos - start_time) / (end_time - start_time)) * w as f64) as usize;
                    let color = match i {
                        0 => colors::RED,
                        1 => colors::GREEN,
                        2 => colors::BLUE,
                        _ => colors::CYAN,
                    };
                    buffer.draw_vline(hc_x, y, 8, color);
                }
            }
        }
    }

    /// Render track title and artist.
    fn render_track_info(
        &self,
        buffer: &mut FrameBuffer,
        deck: &DeckDisplayState,
        x: usize,
        y: usize,
    ) {
        if deck.title.is_empty() {
            return;
        }

        // Truncate title if too long
        let max_chars = (DECK_WIDTH - 10) / 6; // 6 pixels per char at scale 1
        let title = if deck.title.len() > max_chars {
            format!("{}...", &deck.title[..max_chars - 3])
        } else {
            deck.title.clone()
        };

        buffer.draw_text(x + 4, y, &title, colors::WHITE, 2);

        // Artist (smaller, below title)
        if !deck.artist.is_empty() {
            let artist = if deck.artist.len() > max_chars {
                format!("{}...", &deck.artist[..max_chars - 3])
            } else {
                deck.artist.clone()
            };
            buffer.draw_text(x + 4, y + 18, &artist, colors::GRAY, 1);
        }
    }

    /// Render transport state (play/pause, sync, master).
    fn render_transport(
        &self,
        buffer: &mut FrameBuffer,
        deck: &DeckDisplayState,
        x: usize,
        y: usize,
        position: f64,
    ) {
        // Time display using interpolated position
        let pos_min = (position / 60.0) as u32;
        let pos_sec = (position % 60.0) as u32;
        let dur_min = (deck.duration_seconds / 60.0) as u32;
        let dur_sec = (deck.duration_seconds % 60.0) as u32;

        let time_str = format!(
            "{:02}:{:02} / {:02}:{:02}",
            pos_min, pos_sec, dur_min, dur_sec
        );
        buffer.draw_text(x + 4, y, &time_str, colors::WHITE, 1);

        // Play/Pause indicator
        let transport_y = y + 12;
        if deck.is_playing {
            buffer.draw_text(x + 4, transport_y, "PLAY", colors::GREEN, 1);
        } else {
            buffer.draw_text(x + 4, transport_y, "PAUSE", colors::ORANGE, 1);
        }

        // Sync indicator
        if deck.sync_enabled {
            buffer.draw_text(x + 60, transport_y, "SYNC", colors::CYAN, 1);
        }

        // Master indicator
        if deck.is_master {
            buffer.draw_text(x + 110, transport_y, "MASTER", colors::ORANGE, 1);
        }
    }

    /// Render BPM display.
    fn render_bpm(&self, buffer: &mut FrameBuffer, deck: &DeckDisplayState, x: usize, y: usize) {
        if deck.bpm > 0.0 {
            let bpm_str = format!("{:.2} BPM", deck.bpm);
            buffer.draw_text(x + 4, y, &bpm_str, colors::WHITE, 2);
        }
    }
}

impl Default for DisplayRenderer {
    fn default() -> Self {
        Self::new()
    }
}
