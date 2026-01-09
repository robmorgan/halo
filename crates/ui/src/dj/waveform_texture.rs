//! GPU-accelerated waveform texture rendering.
//!
//! Pre-renders waveform data to a GPU texture for efficient scrolling display.
//! Instead of drawing thousands of line segments per frame, we render once to
//! a texture and then just scroll/clip the texture (GPU handles this efficiently).

use std::sync::Arc;

use eframe::egui::{self, Color32, ColorImage, TextureHandle, TextureOptions};

/// Height of the rendered waveform texture in pixels.
/// Using a taller texture for better vertical resolution when zoomed.
const TEXTURE_HEIGHT: usize = 256;

/// Maximum texture width - balances quality vs memory.
/// 8000 pixels allows ~26 pixels per second for a 5-minute track.
const MAX_TEXTURE_WIDTH: usize = 8000;

/// Cached GPU texture for waveform display.
pub struct WaveformTexture {
    /// The GPU texture handle.
    texture: Option<TextureHandle>,
    /// Pointer to the waveform data this texture was rendered from.
    /// Used to detect when we need to re-render.
    waveform_ptr: usize,
    /// Pointer to the color data this texture was rendered from.
    colors_ptr: usize,
    /// Number of samples in the waveform when texture was created.
    sample_count: usize,
    /// Width of the texture in pixels.
    texture_width: usize,
}

impl Default for WaveformTexture {
    fn default() -> Self {
        Self {
            texture: None,
            waveform_ptr: 0,
            colors_ptr: 0,
            sample_count: 0,
            texture_width: 0,
        }
    }
}

impl WaveformTexture {
    /// Check if the texture needs to be regenerated based on waveform data changes.
    pub fn needs_update(
        &self,
        waveform: &Arc<Vec<f32>>,
        colors: &Option<Arc<Vec<(f32, f32, f32)>>>,
    ) -> bool {
        let waveform_ptr = Arc::as_ptr(waveform) as usize;
        let colors_ptr = colors
            .as_ref()
            .map(|c| Arc::as_ptr(c) as usize)
            .unwrap_or(0);

        self.texture.is_none()
            || self.waveform_ptr != waveform_ptr
            || self.colors_ptr != colors_ptr
            || self.sample_count != waveform.len()
    }

    /// Render waveform data to a GPU texture.
    ///
    /// This is called once when the waveform changes, not every frame.
    pub fn update(
        &mut self,
        ctx: &egui::Context,
        waveform: &Arc<Vec<f32>>,
        colors: &Option<Arc<Vec<(f32, f32, f32)>>>,
        texture_width: usize,
    ) {
        if waveform.is_empty() || texture_width == 0 {
            self.texture = None;
            self.sample_count = 0;
            return;
        }

        // Track what data we rendered from
        self.waveform_ptr = Arc::as_ptr(waveform) as usize;
        self.colors_ptr = colors
            .as_ref()
            .map(|c| Arc::as_ptr(c) as usize)
            .unwrap_or(0);
        self.sample_count = waveform.len();
        self.texture_width = texture_width;

        // Create the image data
        let mut pixels = vec![Color32::TRANSPARENT; texture_width * TEXTURE_HEIGHT];
        let mid_y = TEXTURE_HEIGHT / 2;
        let num_samples = waveform.len();
        let samples_per_pixel = num_samples as f32 / texture_width as f32;

        for x in 0..texture_width {
            let sample_idx = (x as f32 * samples_per_pixel) as usize;
            if sample_idx < num_samples {
                let amplitude = waveform[sample_idx].abs();
                let height = (amplitude * (TEXTURE_HEIGHT / 2) as f32 * 0.95) as usize;

                // Get color for this sample
                let color = if let Some(ref color_data) = colors {
                    if sample_idx < color_data.len() {
                        let (low, mid, high) = color_data[sample_idx];
                        frequency_bands_to_color(low, mid, high)
                    } else {
                        gradient_color(sample_idx as f64 / num_samples as f64)
                    }
                } else {
                    gradient_color(sample_idx as f64 / num_samples as f64)
                };

                // Draw vertical line (symmetric around center)
                for dy in 0..=height {
                    if mid_y + dy < TEXTURE_HEIGHT {
                        pixels[(mid_y + dy) * texture_width + x] = color;
                    }
                    if mid_y >= dy {
                        pixels[(mid_y - dy) * texture_width + x] = color;
                    }
                }
            }
        }

        // Create the texture
        let size = [texture_width, TEXTURE_HEIGHT];
        let image = ColorImage {
            size,
            pixels,
            source_size: egui::Vec2::new(texture_width as f32, TEXTURE_HEIGHT as f32),
        };

        self.texture = Some(ctx.load_texture(
            "waveform",
            image,
            TextureOptions {
                // Use Nearest filtering for crisp, sharp waveform pixels
                magnification: egui::TextureFilter::Nearest,
                minification: egui::TextureFilter::Nearest,
                ..Default::default()
            },
        ));
    }

    /// Draw the overview waveform (full track visible).
    ///
    /// The playhead position is indicated separately; this just draws the waveform.
    pub fn draw_overview(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        if let Some(ref texture) = self.texture {
            // Draw the full texture scaled to fit the rect
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            ui.painter().image(texture.id(), rect, uv, Color32::WHITE);
        }
    }

    /// Draw the zoomed waveform (scrolling CDJ-style view).
    ///
    /// Shows a portion of the waveform centered around the playhead position.
    pub fn draw_zoomed(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        position_seconds: f64,
        duration_seconds: f64,
        visible_duration: f64,
        playhead_ratio: f32,
    ) {
        if let Some(ref texture) = self.texture {
            if duration_seconds <= 0.0 {
                return;
            }

            // Calculate the visible time window
            let window_start = position_seconds - (visible_duration * playhead_ratio as f64);
            let window_end = window_start + visible_duration;

            // Convert to UV coordinates (0.0 to 1.0)
            let uv_start = (window_start / duration_seconds).clamp(0.0, 1.0) as f32;
            let uv_end = (window_end / duration_seconds).clamp(0.0, 1.0) as f32;

            // Handle edge cases where window extends beyond track
            let uv = egui::Rect::from_min_max(egui::pos2(uv_start, 0.0), egui::pos2(uv_end, 1.0));

            // Calculate the visible portion of the rect when window extends beyond track
            let visible_start_ratio = if window_start < 0.0 {
                (-window_start / visible_duration) as f32
            } else {
                0.0
            };
            let visible_end_ratio = if window_end > duration_seconds {
                1.0 - ((window_end - duration_seconds) / visible_duration) as f32
            } else {
                1.0
            };

            let draw_rect = egui::Rect::from_min_max(
                egui::pos2(rect.left() + rect.width() * visible_start_ratio, rect.top()),
                egui::pos2(
                    rect.left() + rect.width() * visible_end_ratio,
                    rect.bottom(),
                ),
            );

            ui.painter()
                .image(texture.id(), draw_rect, uv, Color32::WHITE);
        }
    }

    /// Returns true if a texture is loaded.
    pub fn has_texture(&self) -> bool {
        self.texture.is_some()
    }
}

/// Convert 3-band frequency data to RGB color (CDJ/rekordbox style).
fn frequency_bands_to_color(low: f32, mid: f32, high: f32) -> Color32 {
    let scale = 2.5;
    let r = (low * scale * 255.0).clamp(0.0, 255.0) as u8;
    let g = (mid * scale * 255.0).clamp(0.0, 255.0) as u8;
    let b = (high * scale * 255.0).clamp(0.0, 255.0) as u8;
    Color32::from_rgb(r, g, b)
}

/// Get gradient color for waveform based on position (fallback).
fn gradient_color(progress: f64) -> Color32 {
    let r = (100.0 + progress * 155.0) as u8;
    let g = (200.0 - progress * 100.0) as u8;
    let b = 255;
    Color32::from_rgb(r, g, b)
}
