//! Library-row waveform thumbnail: rasterizes the compact per-track
//! amplitude blob (3 bands × N u8 columns, band-major low/mid/high,
//! produced by the analysis worker) into the same bottom-anchored 3-band
//! silhouette as the deck overview, at full height (no cue headroom).

use eframe::egui;

use super::palette;

/// Thumbnail texture height in pixels (2x the ~20pt cell for retina).
const THUMB_TEX_HEIGHT: usize = 40;
/// Perceptual lift, matching the overview strip.
const THUMB_GAMMA: f32 = 0.85;

/// Rasterizes a thumbnail blob into a `[N, THUMB_TEX_HEIGHT]` image, one
/// texture column per blob column (`TextureOptions::LINEAR` handles the
/// minify at draw time). `None` if the blob is malformed (empty or not
/// divisible into 3 bands). Bands paint high → mid → low — low on top:
/// see `overview::render_level`.
pub fn render_thumbnail(data: &[u8]) -> Option<egui::ColorImage> {
    if data.is_empty() || !data.len().is_multiple_of(3) {
        return None;
    }
    let n = data.len() / 3;
    let mut image = egui::ColorImage::new([n, THUMB_TEX_HEIGHT], egui::Color32::TRANSPARENT);
    let band_colors = [palette::BAND_LOW, palette::BAND_MID, palette::BAND_HIGH];
    for x in 0..n {
        for (band, &color) in band_colors.iter().enumerate().rev() {
            let amp = (data[band * n + x] as f32 / 255.0).powf(THUMB_GAMMA);
            paint_silhouette_column(&mut image, x, amp, 1.0, color);
        }
    }
    Some(image)
}

/// Paints one bottom-anchored silhouette column: a solid run of `color`
/// rising `amp * height_frac` of the image height from the baseline, with
/// the partial pixel at the top composited over what's already there at
/// its coverage (anti-aliased edge). Shared by the overview strip and the
/// library thumbnails.
pub(super) fn paint_silhouette_column(
    image: &mut egui::ColorImage,
    x: usize,
    amp: f32,
    height_frac: f32,
    color: egui::Color32,
) {
    let [width, height] = image.size;
    let top_f = (1.0 - amp * height_frac) * height as f32;
    let top = top_f.ceil().clamp(0.0, height as f32) as usize;
    for y in top..height {
        image.pixels[y * width + x] = color;
    }
    let coverage = top as f32 - top_f;
    if coverage > 0.0 && top > 0 {
        let dst = &mut image.pixels[(top - 1) * width + x];
        let inv = 1.0 - coverage;
        *dst = egui::Color32::from_rgba_premultiplied(
            (color.r() as f32 * coverage + dst.r() as f32 * inv).round() as u8,
            (color.g() as f32 * coverage + dst.g() as f32 * inv).round() as u8,
            (color.b() as f32 * coverage + dst.b() as f32 * inv).round() as u8,
            (255.0 * coverage + dst.a() as f32 * inv).round() as u8,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_blobs_read_none() {
        assert!(render_thumbnail(&[]).is_none());
        assert!(render_thumbnail(&[10, 20]).is_none());
        assert!(render_thumbnail(&[10, 20, 30, 40]).is_none());
    }

    #[test]
    fn valid_blob_renders_at_blob_width() {
        let image = render_thumbnail(&[128; 3 * 256]).unwrap();
        assert_eq!(image.size, [256, THUMB_TEX_HEIGHT]);
    }

    #[test]
    fn silent_blob_is_fully_transparent() {
        let image = render_thumbnail(&[0; 3 * 8]).unwrap();
        assert!(
            image
                .pixels
                .iter()
                .all(|&p| p == egui::Color32::TRANSPARENT)
        );
    }

    #[test]
    fn full_scale_low_band_fills_the_column() {
        // Low at 255, mid/high silent: the whole column is BAND_LOW.
        let mut data = vec![0u8; 3 * 4];
        data[0..4].fill(255);
        let image = render_thumbnail(&data).unwrap();
        for y in 0..THUMB_TEX_HEIGHT {
            assert_eq!(image.pixels[y * 4], palette::BAND_LOW);
        }
    }
}
