//! The Options modal: Halo's settings, edited as a draft over the running
//! app. Playback, meters and lighting carry on behind it; the backdrop takes
//! every click and the app's shortcuts stay off while it's open. SAVE hands
//! the draft back to be applied, CANCEL or Esc drops it.

use std::net::IpAddr;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Id, Layout, Margin, PopupCloseBehavior, Rect,
    Sense, Stroke, Ui, UiBuilder, Vec2, pos2, vec2,
};
use halo_light::artnet::ARTNET_PORT;

use crate::key::{self, Key, Lit};
use crate::theme::{self, Face, TypeStyle};

/// The values Options edits. Only these are persisted, never the lists
/// they're chosen from (the audio devices are enumerated on open).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionsValues {
    /// Output device by name; `None` follows the system default.
    pub device_name: Option<String>,
    /// Buffer size in frames; `None` lets the device choose.
    pub buffer_size: Option<u32>,
    /// Art-Net to one node (`node_ip`) instead of broadcast.
    pub unicast: bool,
    /// The unicast node's address, kept while broadcasting.
    pub node_ip: String,
}

/// What closing the modal asks the app to do.
pub enum Outcome {
    Save(OptionsValues),
    Cancel,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Category {
    Audio,
    Lighting,
}

impl Category {
    const ALL: [Category; 2] = [Category::Audio, Category::Lighting];

    fn label(self) -> &'static str {
        match self {
            Category::Audio => "AUDIO",
            Category::Lighting => "LIGHTING",
        }
    }
}

/// The buffer sizes on offer, in frames.
const BUFFER_SIZES: [u32; 6] = [64, 128, 256, 512, 1024, 2048];

/// The open modal's state: the values as saved, the draft being edited,
/// and what the draft's controls choose from.
pub struct Options {
    saved: OptionsValues,
    draft: OptionsValues,
    category: Category,
    devices: Vec<String>,
    /// The running stream's rate, for the latency readout.
    sample_rate: Option<u32>,
}

// Layout, in points.
const MODAL_SIZE: Vec2 = vec2(880.0, 620.0);
/// The least room kept round the modal when the window is small.
const MODAL_MARGIN: f32 = 16.0;
/// The backdrop: `ground` at 62%.
const BACKDROP_ALPHA: f32 = 0.62;
const RAIL_W: f32 = 200.0;
/// The rail's and the pane's title bands, and the footer.
const BAND_H: f32 = 56.0;
const RAIL_TITLE_PAD: f32 = 20.0;
const CATEGORY_H: f32 = 40.0;
const CATEGORY_GAP: f32 = 2.0;
const CATEGORY_INSET: f32 = 8.0;
const CATEGORY_PAD: f32 = 12.0;
const PANE_TITLE_PAD: f32 = 24.0;
const SETTINGS_PAD_TOP: f32 = 16.0;
const SETTINGS_PAD: f32 = 24.0;
const GROUP_GAP: f32 = 24.0;
const GROUP_HEADING_GAP: f32 = 6.0;
const ROW_MIN_H: f32 = 52.0;
const ROW_PAD: f32 = 8.0;
const LABEL_GAP: f32 = 16.0;
const CONTROL_W: f32 = 272.0;
const CONTROL_H: f32 = 30.0;
const NOTE_GAP: f32 = 3.0;
const READOUT_W: f32 = 96.0;
/// Text inside a dropdown key or a field, from its edges.
const FIELD_PAD: f32 = 12.0;
const ARROW: TypeStyle = theme::BODY_TEXT.sized(9.0);
const POPUP_PAD: i8 = 3;
const POPUP_ROW_H: f32 = 30.0;
const POPUP_ROW_PAD: f32 = 9.0;
const FOOTER_PAD_LEFT: f32 = 20.0;
const FOOTER_PAD_RIGHT: f32 = 16.0;
const FOOTER_KEY: Vec2 = vec2(96.0, 30.0);
const FOOTER_KEY_GAP: f32 = 8.0;

impl Options {
    pub fn new(saved: OptionsValues, devices: Vec<String>, sample_rate: Option<u32>) -> Self {
        Self {
            draft: saved.clone(),
            saved,
            category: Category::Audio,
            devices,
            sample_rate,
        }
    }

    /// Unsaved changes in one category.
    fn changes(&self, category: Category) -> usize {
        let (draft, saved) = (&self.draft, &self.saved);
        match category {
            Category::Audio => {
                usize::from(draft.device_name != saved.device_name)
                    + usize::from(draft.buffer_size != saved.buffer_size)
            }
            Category::Lighting => {
                usize::from(draft.unicast != saved.unicast)
                    + usize::from(draft.unicast && draft.node_ip != saved.node_ip)
            }
        }
    }

    fn total_changes(&self) -> usize {
        Category::ALL.iter().map(|&c| self.changes(c)).sum()
    }

    /// A unicast draft needs an address that parses.
    fn node_ip_valid(&self) -> bool {
        !self.draft.unicast || self.draft.node_ip.trim().parse::<IpAddr>().is_ok()
    }

    /// Draw the modal; returns what to do once SAVE, CANCEL or Esc closes it.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Outcome> {
        // Esc closes an open dropdown (the popup handles that itself) and
        // only cancels when none was open.
        let popup_was_open = ctx.memory(|m| m.any_popup_open());
        let screen = ctx.screen_rect().size();
        let size = MODAL_SIZE
            .min(screen - Vec2::splat(2.0 * MODAL_MARGIN))
            .max(Vec2::ZERO);
        let mut outcome = None;
        let modal = egui::Modal::new(Id::new("options"))
            .backdrop_color(theme::GROUND.gamma_multiply(BACKDROP_ALPHA))
            .frame(
                egui::Frame::NONE
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0, theme::EDGE))
                    .corner_radius(theme::RADIUS_WINDOW),
            )
            .show(ctx, |ui| self.body(ui, size, &mut outcome));
        if outcome.is_none()
            && modal.is_top_modal
            && !popup_was_open
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            outcome = Some(Outcome::Cancel);
        }
        outcome
    }

    fn body(&mut self, ui: &mut Ui, size: Vec2, outcome: &mut Option<Outcome>) {
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let footer = Rect::from_min_max(pos2(rect.left(), rect.bottom() - BAND_H), rect.max);
        let rail = Rect::from_min_max(rect.min, pos2(rect.left() + RAIL_W, footer.top()));
        let pane = Rect::from_min_max(
            pos2(rail.right(), rect.top()),
            pos2(rect.right(), footer.top()),
        );
        let clip = ui.clip_rect().intersect(rect);
        let painter = ui.painter().with_clip_rect(clip);
        let line = Stroke::new(1.0, theme::EDGE_PANEL);

        // Rail: the well, its title and the categories.
        painter.rect_filled(
            rail,
            CornerRadius {
                nw: theme::RADIUS_WINDOW,
                ..CornerRadius::ZERO
            },
            theme::WELL,
        );
        painter.vline(rail.right() - 0.5, rail.y_range(), line);
        theme::paint_text(
            &painter,
            pos2(rail.left() + RAIL_TITLE_PAD, rail.top() + BAND_H / 2.0),
            Align2::LEFT_CENTER,
            "OPTIONS",
            theme::TRACK_TITLE,
            theme::TEXT_STRONG,
        );
        for (i, category) in Category::ALL.into_iter().enumerate() {
            let top = rail.top() + BAND_H + i as f32 * (CATEGORY_H + CATEGORY_GAP);
            let row = Rect::from_min_size(
                pos2(rail.left() + CATEGORY_INSET, top),
                vec2(RAIL_W - 2.0 * CATEGORY_INSET, CATEGORY_H),
            );
            let resp = ui.interact(row, ui.id().with(category.label()), Sense::click());
            let selected = self.category == category;
            if selected {
                painter.rect_filled(row, theme::RADIUS_CONTROL, theme::SELECTION);
            } else if resp.hovered() {
                theme::paint_face(
                    &painter,
                    row,
                    theme::RADIUS_CONTROL,
                    Face::KeyHover,
                    Color32::WHITE,
                );
            }
            theme::paint_text(
                &painter,
                pos2(row.left() + CATEGORY_PAD, row.center().y),
                Align2::LEFT_CENTER,
                category.label(),
                theme::TEXT_TOGGLE,
                if selected {
                    theme::TEXT_STRONG
                } else {
                    theme::TEXT
                },
            );
            let changes = self.changes(category);
            if changes > 0 {
                theme::paint_text(
                    &painter,
                    pos2(row.right() - CATEGORY_PAD, row.center().y),
                    Align2::RIGHT_CENTER,
                    changes,
                    theme::NUMERALS_SM,
                    theme::ACCENT,
                );
            }
            if resp.has_focus() {
                theme::paint_focus(&painter, row);
            }
            if resp.clicked() {
                self.category = category;
            }
        }

        // Pane: the category's title, then its settings, the only part
        // that scrolls.
        theme::paint_text(
            &painter,
            pos2(pane.left() + PANE_TITLE_PAD, pane.top() + BAND_H / 2.0),
            Align2::LEFT_CENTER,
            self.category.label(),
            theme::TRACK_TITLE,
            theme::TEXT_STRONG,
        );
        painter.hline(pane.x_range(), pane.top() + BAND_H - 0.5, line);
        let settings = Rect::from_min_max(pos2(pane.left(), pane.top() + BAND_H), pane.max);
        let mut settings_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(settings)
                .layout(Layout::top_down(Align::Min)),
        );
        settings_ui.set_clip_rect(settings.intersect(clip));
        egui::ScrollArea::vertical()
            .id_salt(("options_settings", self.category.label()))
            .auto_shrink([false, false])
            .show(&mut settings_ui, |ui| {
                egui::Frame::NONE
                    .inner_margin(Margin {
                        left: SETTINGS_PAD as i8,
                        right: SETTINGS_PAD as i8,
                        top: SETTINGS_PAD_TOP as i8,
                        bottom: SETTINGS_PAD as i8,
                    })
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        match self.category {
                            Category::Audio => self.audio(ui),
                            Category::Lighting => self.lighting(ui),
                        }
                    });
            });

        // Footer: what's unsaved, then CANCEL and SAVE.
        painter.hline(footer.x_range(), footer.top() + 0.5, line);
        let changes = self.total_changes();
        let valid = self.node_ip_valid();
        let (note, color) = if !valid {
            (
                "Node address isn't a valid IP address.".to_string(),
                theme::WARN,
            )
        } else if changes > 0 {
            let noun = if changes == 1 { "change" } else { "changes" };
            (format!("{changes} {noun} not saved"), theme::ACCENT)
        } else {
            (
                "Playback and lighting keep running while Options is open.".to_string(),
                theme::TEXT_MUTED,
            )
        };
        theme::paint_text(
            &painter,
            pos2(footer.left() + FOOTER_PAD_LEFT, footer.center().y),
            Align2::LEFT_CENTER,
            note,
            theme::NOTE_TEXT,
            color,
        );
        let save = Rect::from_min_size(
            pos2(
                footer.right() - FOOTER_PAD_RIGHT - FOOTER_KEY.x,
                footer.center().y - FOOTER_KEY.y / 2.0,
            ),
            FOOTER_KEY,
        );
        let cancel = save.translate(vec2(-(FOOTER_KEY.x + FOOTER_KEY_GAP), 0.0));
        if ui
            .put(cancel, Key::new("CANCEL").size(FOOTER_KEY))
            .clicked()
        {
            *outcome = Some(Outcome::Cancel);
        }
        let save_key = Key::new("SAVE")
            .size(FOOTER_KEY)
            .label_color(theme::ACCENT)
            .lit((changes > 0).then_some(Lit::Accent))
            .enabled(valid);
        if ui.put(save, save_key).clicked() {
            *outcome = Some(Outcome::Save(self.draft.clone()));
        }
    }

    fn audio(&mut self, ui: &mut Ui) {
        group(ui, "OUTPUT", |ui| {
            row(
                ui,
                "OUTPUT DEVICE",
                Some("Changing it silences audio briefly; playing decks carry on."),
                |ui| {
                    let mut choices = vec![("SYSTEM DEFAULT".to_string(), None)];
                    choices.extend(self.devices.iter().map(|d| (d.clone(), Some(d.clone()))));
                    // A saved device that's unplugged stays choosable.
                    if let Some(saved) = &self.saved.device_name
                        && !self.devices.contains(saved)
                    {
                        choices.push((format!("{saved} (not connected)"), Some(saved.clone())));
                    }
                    if let Some(choice) =
                        dropdown(ui, "output_device", &choices, &self.draft.device_name)
                    {
                        self.draft.device_name = choice;
                    }
                },
            );
            row(
                ui,
                "BUFFER SIZE",
                Some("Smaller is quicker to respond, larger is safer from dropouts."),
                |ui| {
                    let mut choices = vec![("DEVICE DEFAULT".to_string(), None)];
                    choices.extend(BUFFER_SIZES.map(|n| (format!("{n} FRAMES"), Some(n))));
                    if let Some(choice) =
                        dropdown(ui, "buffer_size", &choices, &self.draft.buffer_size)
                    {
                        self.draft.buffer_size = choice;
                    }
                },
            );
            let rate = self.sample_rate;
            let note = match (self.draft.buffer_size, rate) {
                (None, _) => "The device sets it at its default buffer size.".to_string(),
                (Some(_), Some(rate)) => {
                    let khz = rate as f32 / 1000.0;
                    format!("Buffer size ÷ sample rate, at {khz} kHz now.")
                }
                (Some(_), None) => "No output is running.".to_string(),
            };
            row(ui, "OUTPUT LATENCY", Some(&note), |ui| {
                let value = match (self.draft.buffer_size, rate) {
                    (Some(frames), Some(rate)) => {
                        format!("{:.1} MS", frames as f32 * 1000.0 / rate as f32)
                    }
                    _ => "—".to_string(),
                };
                readout(ui, &value);
            });
        });
    }

    fn lighting(&mut self, ui: &mut Ui) {
        group(ui, "ART-NET OUTPUT", |ui| {
            let note = if self.draft.unicast {
                "Sends to one node only."
            } else {
                "Sends to every node on the network."
            };
            row(ui, "DESTINATION", Some(note), |ui| {
                let w = CONTROL_W / 2.0;
                let [broadcast, unicast] = key::segmented(
                    ui,
                    [
                        Key::new("BROADCAST")
                            .size(vec2(w, CONTROL_H))
                            .pressed(!self.draft.unicast),
                        Key::new("UNICAST")
                            .size(vec2(w, CONTROL_H))
                            .pressed(self.draft.unicast),
                    ],
                );
                if broadcast.clicked() {
                    self.draft.unicast = false;
                }
                if unicast.clicked() {
                    self.draft.unicast = true;
                }
            });
            let note = if !self.draft.unicast {
                "Used when the destination is unicast.".to_string()
            } else if self.node_ip_valid() {
                format!("Art-Net port {ARTNET_PORT}.")
            } else {
                "Enter an IPv4 or IPv6 address.".to_string()
            };
            let (unicast, valid) = (self.draft.unicast, self.node_ip_valid());
            row(ui, "NODE ADDRESS", Some(&note), |ui| {
                text_field(ui, &mut self.draft.node_ip, unicast, valid);
            });
        });
    }
}

/// A titled group of rows: a `caption` heading over an `edge-panel` line.
/// Groups after the first start `GROUP_GAP` below the one before.
fn group(ui: &mut Ui, heading: &str, rows: impl FnOnce(&mut Ui)) {
    if ui.min_rect().height() > 0.0 {
        ui.add_space(GROUP_GAP);
    }
    let width = ui.available_width();
    let heading_h = theme::CAPTION
        .galley(ui.ctx(), heading, theme::TEXT_MUTED)
        .size()
        .y;
    let (rect, _) = ui.allocate_exact_size(vec2(width, heading_h), Sense::hover());
    theme::paint_text(
        ui.painter(),
        rect.left_top(),
        Align2::LEFT_TOP,
        heading,
        theme::CAPTION,
        theme::TEXT_MUTED,
    );
    ui.add_space(GROUP_HEADING_GAP);
    let (rule, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
    ui.painter().hline(
        rule.x_range(),
        rule.center().y,
        Stroke::new(1.0, theme::EDGE_PANEL),
    );
    rows(ui);
}

/// One setting: its label (and an optional one-line note under it) filling
/// the row, then the control in a fixed column, a hairline under it all.
fn row(ui: &mut Ui, label: &str, note: Option<&str>, control: impl FnOnce(&mut Ui)) {
    let width = ui.available_width();
    let label_w = (width - LABEL_GAP - CONTROL_W).max(0.0);
    let ctx = ui.ctx().clone();
    let label_galley = truncated(&ctx, label, theme::TEXT_TOGGLE, theme::TEXT, label_w);
    let note_galley =
        note.map(|n| truncated(&ctx, n, theme::NOTE_TEXT, theme::TEXT_MUTED, label_w));
    let label_h =
        label_galley.size().y + note_galley.as_ref().map_or(0.0, |g| NOTE_GAP + g.size().y);
    let height = (2.0 * ROW_PAD + label_h.max(CONTROL_H)).max(ROW_MIN_H);
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let mid = rect.center().y;

    let painter = ui.painter();
    let label_top = mid - label_h / 2.0;
    let label_size = label_galley.size();
    painter.galley(pos2(rect.left(), label_top), label_galley, theme::TEXT);
    if let Some(galley) = note_galley {
        painter.galley(
            pos2(rect.left(), label_top + label_size.y + NOTE_GAP),
            galley,
            theme::TEXT_MUTED,
        );
    }
    painter.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, theme::QUIET_HAIRLINE),
    );

    let column = Rect::from_min_size(
        pos2(rect.right() - CONTROL_W, mid - CONTROL_H / 2.0),
        vec2(CONTROL_W, CONTROL_H),
    );
    let mut control_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(column)
            .layout(Layout::left_to_right(Align::Center)),
    );
    control(&mut control_ui);
}

/// One line of `text`, cut short with "…" at `width`.
fn truncated(
    ctx: &egui::Context,
    text: &str,
    style: TypeStyle,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(text.to_string(), style.format(color));
    job.wrap = egui::text::TextWrapping::truncate_at_width(width);
    ctx.fonts(|f| f.layout_job(job))
}

/// A long list: a key showing the current value with a ▼, opening a list
/// in a well below it. Returns the value picked this frame.
fn dropdown<T: Clone + PartialEq>(
    ui: &mut Ui,
    id_salt: &str,
    choices: &[(String, T)],
    current: &T,
) -> Option<T> {
    let popup_id = ui.make_persistent_id(id_salt);
    let open = ui.memory(|m| m.is_popup_open(popup_id));
    let (rect, resp) = ui.allocate_exact_size(vec2(CONTROL_W, CONTROL_H), Sense::click());
    let painter = ui.painter();
    let r = theme::RADIUS_CONTROL;
    theme::paint_edge(painter, rect, r, theme::EDGE);
    if open || resp.is_pointer_button_down_on() {
        theme::paint_key_face(
            painter,
            rect,
            r,
            Face::KeyDown,
            Color32::WHITE,
            theme::Top::Shade,
        );
    } else {
        let face = if resp.hovered() {
            Face::KeyHover
        } else {
            Face::Key
        };
        theme::paint_key_face(
            painter,
            rect,
            r,
            face,
            Color32::WHITE,
            theme::Top::Sheen(theme::SHEEN),
        );
    }
    let arrow = theme::paint_text(
        painter,
        pos2(rect.right() - FIELD_PAD, rect.center().y),
        Align2::RIGHT_CENTER,
        "▼",
        ARROW,
        theme::TEXT_MUTED,
    );
    let label = choices
        .iter()
        .find(|(_, value)| value == current)
        .map_or("", |(label, _)| label.as_str());
    let value_w = arrow.left() - FIELD_PAD - (rect.left() + FIELD_PAD);
    let galley = truncated(
        ui.ctx(),
        label,
        theme::TEXT_TOGGLE,
        theme::TEXT_STRONG,
        value_w,
    );
    painter.galley(
        pos2(
            rect.left() + FIELD_PAD,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        theme::TEXT_STRONG,
    );
    if resp.has_focus() {
        theme::paint_focus(painter, rect);
    }
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }

    // The list: a well with an `edge` hairline, the current value washed in
    // `selection`.
    let mut picked = None;
    ui.scope(|ui| {
        let style = ui.style_mut();
        style.visuals.window_fill = theme::WELL;
        style.visuals.window_stroke = Stroke::new(1.0, theme::EDGE);
        style.visuals.popup_shadow = egui::Shadow::NONE;
        style.visuals.menu_corner_radius = CornerRadius::same(r);
        style.spacing.menu_margin = Margin::same(POPUP_PAD);
        egui::popup_below_widget(
            ui,
            popup_id,
            &resp,
            PopupCloseBehavior::CloseOnClick,
            |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let width = CONTROL_W - 2.0 * POPUP_PAD as f32;
                ui.set_width(width);
                for (label, value) in choices {
                    let (row, resp) =
                        ui.allocate_exact_size(vec2(width, POPUP_ROW_H), Sense::click());
                    let painter = ui.painter();
                    if value == current {
                        painter.rect_filled(row, r, theme::SELECTION);
                    } else if resp.hovered() {
                        theme::paint_face(painter, row, r, Face::KeyHover, Color32::WHITE);
                    }
                    let galley = truncated(
                        ui.ctx(),
                        label,
                        theme::TEXT_TOGGLE,
                        if value == current {
                            theme::TEXT_STRONG
                        } else {
                            theme::TEXT
                        },
                        width - 2.0 * POPUP_ROW_PAD,
                    );
                    let color = galley.job.sections[0].format.color;
                    painter.galley(
                        pos2(
                            row.left() + POPUP_ROW_PAD,
                            row.center().y - galley.size().y / 2.0,
                        ),
                        galley,
                        color,
                    );
                    if resp.clicked() {
                        picked = Some(value.clone());
                    }
                }
            },
        );
    });
    picked
}

/// A read-only value: a 96-wide well, the value right-aligned.
fn readout(ui: &mut Ui, value: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(READOUT_W, CONTROL_H), Sense::hover());
    theme::paint_well(ui.painter(), rect, theme::RADIUS_CONTROL);
    theme::paint_text(
        ui.painter(),
        pos2(rect.right() - FIELD_PAD, rect.center().y),
        Align2::RIGHT_CENTER,
        value,
        theme::NUMERALS_MD,
        theme::TEXT_STRONG,
    );
}

/// A text field: a full-width well, the text in `numerals-md`. An invalid
/// value outlines it in `warn`.
fn text_field(ui: &mut Ui, text: &mut String, enabled: bool, valid: bool) {
    let rect = Rect::from_min_size(ui.cursor().min, vec2(CONTROL_W, CONTROL_H));
    theme::paint_well(ui.painter(), rect, theme::RADIUS_CONTROL);
    let resp = ui.add_enabled(
        enabled,
        egui::TextEdit::singleline(text)
            .font(theme::NUMERALS_MD.font_id())
            .text_color(if enabled {
                theme::TEXT_STRONG
            } else {
                theme::TEXT_DISABLED
            })
            .frame(false)
            .margin(Margin::symmetric(FIELD_PAD as i8, 0))
            .vertical_align(Align::Center)
            .min_size(vec2(CONTROL_W, CONTROL_H))
            .desired_width(CONTROL_W - 2.0 * FIELD_PAD),
    );
    if enabled && !valid {
        ui.painter().rect_stroke(
            rect,
            theme::RADIUS_CONTROL,
            Stroke::new(1.0, theme::WARN),
            egui::StrokeKind::Inside,
        );
    }
    if resp.has_focus() {
        theme::paint_focus(ui.painter(), rect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved() -> OptionsValues {
        OptionsValues {
            device_name: None,
            buffer_size: Some(256),
            unicast: false,
            node_ip: "10.0.0.5".to_string(),
        }
    }

    #[test]
    fn counts_changes_per_category() {
        let mut options = Options::new(saved(), Vec::new(), Some(48_000));
        assert_eq!(options.total_changes(), 0);
        options.draft.buffer_size = Some(512);
        options.draft.device_name = Some("Interface".to_string());
        assert_eq!(options.changes(Category::Audio), 2);
        // The node address only counts while it's the destination.
        options.draft.node_ip = "10.0.0.6".to_string();
        assert_eq!(options.changes(Category::Lighting), 0);
        options.draft.unicast = true;
        assert_eq!(options.changes(Category::Lighting), 2);
        assert_eq!(options.total_changes(), 4);
        // Back to the saved values: nothing to save.
        options.draft = saved();
        assert_eq!(options.total_changes(), 0);
    }

    #[test]
    fn a_unicast_draft_needs_a_valid_address() {
        let mut options = Options::new(saved(), Vec::new(), None);
        options.draft.node_ip = "not an address".to_string();
        assert!(options.node_ip_valid(), "broadcast ignores the address");
        options.draft.unicast = true;
        assert!(!options.node_ip_valid());
        options.draft.node_ip = " 192.168.1.20 ".to_string();
        assert!(options.node_ip_valid());
    }
}
