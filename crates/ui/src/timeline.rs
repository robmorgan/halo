use eframe::egui::{Align2, Color32, FontId, Painter, Rect, Stroke};
use halo_core::{ConsoleCommand, TimeCode};
use tokio::sync::mpsc;

use crate::state::ConsoleState;

#[derive(Debug, Clone)]
pub struct TimelineState {
    pub is_expanded: bool,
}

impl Default for TimelineState {
    fn default() -> Self {
        Self { is_expanded: false }
    }
}

pub fn render(
    ui: &mut eframe::egui::Ui,
    state: &ConsoleState,
    timeline_state: &mut TimelineState,
    console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
) {
    ui.horizontal(|ui| {
        ui.heading("TIMELINE");

        ui.add_space(20.0);

        // Expand/collapse toggle
        let toggle_text = if timeline_state.is_expanded {
            "▼"
        } else {
            "▶"
        };
        if ui.button(toggle_text).clicked() {
            timeline_state.is_expanded = !timeline_state.is_expanded;
        }
    });

    // Expanded timeline view
    if timeline_state.is_expanded {
        ui.add_space(10.0);

        // Allocate space for the timeline
        let timeline_height = 120.0;
        let timeline_response = ui.allocate_rect(
            Rect::from_min_size(
                ui.available_rect_before_wrap().min,
                [ui.available_width(), timeline_height].into(),
            ),
            eframe::egui::Sense::click(),
        );

        // Draw timeline content
        if let Some(waveform_data) = &state.audio_waveform {
            draw_timeline_content(
                &timeline_response,
                ui.painter(),
                waveform_data,
                state,
                console_tx,
            );
        } else {
            // No waveform data - show placeholder
            ui.painter().text(
                timeline_response.rect.center(),
                eframe::egui::Align2::CENTER_CENTER,
                "No audio file loaded",
                eframe::egui::FontId::proportional(16.0),
                Color32::from_rgb(100, 100, 100),
            );
        }
    }
}

fn draw_timeline_content(
    response: &eframe::egui::Response,
    painter: &Painter,
    waveform_data: &halo_core::audio::waveform::WaveformData,
    state: &ConsoleState,
    console_tx: &mpsc::UnboundedSender<ConsoleCommand>,
) {
    let rect = response.rect;
    let width = rect.width();

    // Handle click for needle drop
    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let click_x = pos.x - rect.min.x;
            let time_ratio = (click_x / width).clamp(0.0, 1.0);
            let seek_time = time_ratio as f64 * waveform_data.duration_seconds;
            let _ = console_tx.send(ConsoleCommand::SeekAudio {
                position_seconds: seek_time,
            });
        }
    }

    // Draw waveform
    draw_waveform(painter, rect, waveform_data);

    // Draw cue markers
    draw_cue_markers(painter, rect, state, waveform_data);

    // Draw playback position indicator
    if let Some(timecode) = &state.timecode {
        let current_time = timecode.to_seconds();
        let time_ratio = (current_time / waveform_data.duration_seconds).clamp(0.0, 1.0);
        let position_x = rect.min.x + (time_ratio * width as f64) as f32;

        painter.line_segment(
            [
                eframe::egui::pos2(position_x, rect.min.y),
                eframe::egui::pos2(position_x, rect.max.y),
            ],
            Stroke::new(2.0, Color32::from_rgb(255, 100, 50)),
        );
    }
}

fn draw_waveform(
    painter: &Painter,
    rect: Rect,
    waveform_data: &halo_core::audio::waveform::WaveformData,
) {
    let width = rect.width();
    let height = rect.height();
    let center_y = rect.center().y;
    let samples = &waveform_data.samples;

    if samples.is_empty() {
        return;
    }

    // Draw waveform as filled area with gradient effect
    let mut points = Vec::new();
    let mut bottom_points = Vec::new();

    for (i, &sample) in samples.iter().enumerate() {
        let x = rect.min.x + (i as f32 / samples.len() as f32) * width;
        let amplitude = sample.abs() * (height * 0.4); // Scale amplitude
        let top_y = center_y - amplitude;
        let bottom_y = center_y + amplitude;

        points.push(eframe::egui::pos2(x, top_y));
        bottom_points.push(eframe::egui::pos2(x, bottom_y));
    }

    // Reverse bottom points for closed shape
    bottom_points.reverse();

    // Create closed shape for filled waveform
    let mut shape_points = points.clone();
    shape_points.extend(bottom_points);

    if shape_points.len() >= 3 {
        // Draw filled waveform with gradient effect
        painter.add(eframe::egui::Shape::convex_polygon(
            shape_points,
            Color32::from_rgb(40, 150, 255),
            Stroke::NONE,
        ));

        // Draw waveform outline for better definition
        painter.add(eframe::egui::Shape::line(
            points,
            Stroke::new(1.0, Color32::from_rgb(100, 200, 255)),
        ));
    }
}

/// Extract timecoded cues from the current cue list
fn get_timecoded_cues(state: &ConsoleState) -> Vec<(usize, String, f64)> {
    let mut timecoded_cues = Vec::new();

    if let Some(cue_list) = state.cue_lists.get(state.current_cue_list_index) {
        for (index, cue) in cue_list.cues.iter().enumerate() {
            if let Some(timecode_str) = &cue.timecode {
                let mut timecode = TimeCode::default();
                if timecode.from_string(timecode_str).is_ok() {
                    let seconds = timecode.to_seconds();
                    timecoded_cues.push((index, cue.name.clone(), seconds));
                }
            }
        }
    }

    // Sort by timecode position
    timecoded_cues.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
    timecoded_cues
}

/// Draw cue markers and labels on the timeline
fn draw_cue_markers(
    painter: &Painter,
    rect: Rect,
    state: &ConsoleState,
    waveform_data: &halo_core::audio::waveform::WaveformData,
) {
    let timecoded_cues = get_timecoded_cues(state);

    for (cue_index, cue_name, cue_seconds) in timecoded_cues {
        // Only draw cues that are within the audio duration
        if cue_seconds > waveform_data.duration_seconds {
            continue;
        }

        let time_ratio = (cue_seconds / waveform_data.duration_seconds).clamp(0.0, 1.0);
        let position_x = rect.min.x + (time_ratio * rect.width() as f64) as f32;

        // Draw thin vertical marker line
        painter.line_segment(
            [
                eframe::egui::pos2(position_x, rect.min.y),
                eframe::egui::pos2(position_x, rect.max.y),
            ],
            Stroke::new(1.0, Color32::from_rgb(255, 255, 100)),
        );

        // Draw cue label above the marker
        let label_text = format!("Cue {}: {}", cue_index + 1, cue_name);
        let label_pos = eframe::egui::pos2(position_x, rect.min.y - 5.0);

        painter.text(
            label_pos,
            Align2::CENTER_BOTTOM,
            label_text,
            FontId::proportional(10.0),
            Color32::from_rgb(255, 255, 100),
        );
    }
}
