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

use crate::audio::OutputDevice;
use crate::key::{self, Key, Lit};
use crate::midi::{self, Action, MidiMsg, MidiSettings, Profile};
use crate::theme::{self, Face, TypeStyle};

/// The values Options edits. Only these are persisted, never the lists
/// they're chosen from (the audio devices are enumerated on open).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionsValues {
    /// Output device by name; `None` follows the system default.
    pub device_name: Option<String>,
    /// Buffer size in frames; `None` lets the device choose.
    pub buffer_size: Option<u32>,
    /// Headphone cue on outputs 3–4, on a device that has them.
    pub cue_output: bool,
    /// Art-Net to one node (`node_ip`) instead of broadcast.
    pub unicast: bool,
    /// The unicast node's address, kept while broadcasting.
    pub node_ip: String,
    /// Controller ports, profile and the user's profiles.
    pub midi: MidiSettings,
}

/// The controllers as they are this frame, for the MIDI category.
pub struct MidiLive<'a> {
    pub inputs: &'a [String],
    pub outputs: &'a [String],
    /// Per input: sent something just now.
    pub active: &'a [bool],
    pub messages: &'a [MidiMsg],
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
    Midi,
}

impl Category {
    const ALL: [Category; 3] = [Category::Audio, Category::Lighting, Category::Midi];

    fn label(self) -> &'static str {
        match self {
            Category::Audio => "AUDIO",
            Category::Lighting => "LIGHTING",
            Category::Midi => "MIDI",
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
    devices: Vec<OutputDevice>,
    /// The system default output's name.
    default_device: Option<String>,
    /// The running stream's rate, for the latency readout.
    sample_rate: Option<u32>,
    /// The action waiting for a control to bind to.
    learning: Option<Action>,
    /// The draft profile's SHIFT is held on the draft input.
    learn_shift: bool,
    /// The last message from any controller.
    last: Option<MidiMsg>,
    /// REFRESH was pressed.
    refresh: bool,
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
    pub fn new(
        saved: OptionsValues,
        devices: Vec<OutputDevice>,
        default_device: Option<String>,
        sample_rate: Option<u32>,
    ) -> Self {
        Self {
            draft: saved.clone(),
            saved,
            category: Category::Audio,
            devices,
            default_device,
            sample_rate,
            learning: None,
            learn_shift: false,
            last: None,
            refresh: false,
        }
    }

    /// Waiting for a control to learn: the app holds controller input back.
    pub fn learning(&self) -> bool {
        self.learning.is_some()
    }

    /// The MIDI settings being edited.
    pub fn midi_draft(&self) -> &MidiSettings {
        &self.draft.midi
    }

    /// True once after REFRESH was pressed.
    pub fn take_refresh(&mut self) -> bool {
        std::mem::take(&mut self.refresh)
    }

    /// This frame's controller messages: the last one for the readout, and
    /// the first from the draft input to whatever is learning.
    fn take_messages(&mut self, messages: &[MidiMsg], outputs: &[String]) {
        for m in messages {
            self.last = Some(m.clone());
            let draft = &mut self.draft.midi;
            if draft.input.as_deref().is_some_and(|i| i != &*m.port) {
                continue;
            }
            if draft.profile().and_then(Profile::shift_key) == Some(m.key) {
                self.learn_shift = m.value > 0.0;
                if self.learning != Some(Action::Shift) {
                    continue;
                }
            }
            // A release, or a knob coming to rest at zero, binds nothing.
            let Some(action) = self.learning.filter(|_| m.value > 0.0) else {
                continue;
            };
            // Learning with no input chosen takes the one that answered.
            if draft.input.is_none() {
                let port = m.port.to_string();
                draft.output = draft
                    .output
                    .take()
                    .or_else(|| midi::matching_output(&port, outputs));
                draft.input = Some(port);
            }
            if let Some(profile) = draft.user_profile_mut() {
                profile.learn(action, m.key, self.learn_shift);
            }
            self.learning = None;
        }
    }

    /// Unsaved changes in one category.
    fn changes(&self, category: Category) -> usize {
        let (draft, saved) = (&self.draft, &self.saved);
        match category {
            Category::Audio => {
                usize::from(draft.device_name != saved.device_name)
                    + usize::from(draft.buffer_size != saved.buffer_size)
                    + usize::from(draft.cue_output != saved.cue_output)
            }
            Category::Lighting => {
                usize::from(draft.unicast != saved.unicast)
                    + usize::from(draft.unicast && draft.node_ip != saved.node_ip)
            }
            Category::Midi => {
                let (draft, saved) = (&draft.midi, &saved.midi);
                // Each profile added, removed or edited is one change.
                let user = |settings: &MidiSettings, id: &str| {
                    settings.user_profiles.iter().find(|p| p.id == id).cloned()
                };
                let mut ids: Vec<&str> = draft
                    .user_profiles
                    .iter()
                    .chain(&saved.user_profiles)
                    .map(|p| p.id.as_str())
                    .collect();
                ids.sort_unstable();
                ids.dedup();
                usize::from(draft.input != saved.input)
                    + usize::from(draft.output != saved.output)
                    + usize::from(draft.profile != saved.profile)
                    + ids
                        .into_iter()
                        .filter(|id| user(draft, id) != user(saved, id))
                        .count()
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
    pub fn show(&mut self, ctx: &egui::Context, live: &MidiLive) -> Option<Outcome> {
        self.take_messages(live.messages, live.outputs);
        // Esc stops a learn, closes an open dropdown (the popup handles
        // that itself), and only cancels when neither was going on.
        if self.learning.is_some()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.learning = None;
        }
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
            .show(ctx, |ui| self.body(ui, size, live, &mut outcome));
        if outcome.is_none()
            && modal.is_top_modal
            && !popup_was_open
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            outcome = Some(Outcome::Cancel);
        }
        outcome
    }

    fn body(&mut self, ui: &mut Ui, size: Vec2, live: &MidiLive, outcome: &mut Option<Outcome>) {
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
                self.learning = None;
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
                            Category::Midi => self.midi(ui, live),
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
                    choices.extend(
                        self.devices
                            .iter()
                            .map(|d| (d.name.clone(), Some(d.name.clone()))),
                    );
                    // A saved device that's unplugged stays choosable.
                    if let Some(saved) = &self.saved.device_name
                        && !self.devices.iter().any(|d| &d.name == saved)
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
        group(ui, "HEADPHONES", |ui| {
            let note = match self.draft_device() {
                Some(device) if !device.can_cue() => {
                    format!("No cue: this device has only {} outputs.", device.channels)
                }
                _ => "Plays on outputs 3–4, like the Z1's phones.".to_string(),
            };
            row(ui, "HEADPHONE CUE", Some(&note), |ui| {
                let w = CONTROL_W / 2.0;
                let [on, off] = key::segmented(
                    ui,
                    [
                        Key::new("ON")
                            .size(vec2(w, CONTROL_H))
                            .pressed(self.draft.cue_output),
                        Key::new("OFF")
                            .size(vec2(w, CONTROL_H))
                            .pressed(!self.draft.cue_output),
                    ],
                );
                if on.clicked() {
                    self.draft.cue_output = true;
                }
                if off.clicked() {
                    self.draft.cue_output = false;
                }
            });
        });
    }

    /// The draft's output device, when it's connected.
    fn draft_device(&self) -> Option<&OutputDevice> {
        let name = self
            .draft
            .device_name
            .as_ref()
            .or(self.default_device.as_ref())?;
        self.devices.iter().find(|d| &d.name == name)
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

    fn midi(&mut self, ui: &mut Ui, live: &MidiLive) {
        group(ui, "DEVICES", |ui| {
            let draft = &self.draft.midi;
            for (i, name) in live.inputs.iter().enumerate() {
                let note = if draft.input.as_deref() == Some(name) {
                    "Input. Plays Halo."
                } else {
                    "Input."
                };
                let active = live.active.get(i).copied().unwrap_or(false);
                row(ui, name, Some(note), |ui| activity(ui, active));
            }
            for name in live.outputs {
                let note = if draft.output.as_deref() == Some(name) {
                    "Output. Shows Halo's LEDs and meters."
                } else {
                    "Output."
                };
                row(ui, name, Some(note), |_| {});
            }
            if live.inputs.is_empty() && live.outputs.is_empty() {
                row(ui, "NO DEVICES", Some("Connect a controller."), |_| {});
            }
            row(
                ui,
                "REFRESH",
                Some("Halo also looks for devices every couple of seconds."),
                |ui| {
                    if ui
                        .add(Key::new("REFRESH").size(vec2(READOUT_W, CONTROL_H)))
                        .clicked()
                    {
                        self.refresh = true;
                    }
                },
            );
        });

        group(ui, "CONTROLLER", |ui| {
            row(ui, "INPUT", Some("The device that plays Halo."), |ui| {
                let choices = port_choices(live.inputs, self.saved.midi.input.as_deref());
                if let Some(input) = dropdown(ui, "midi_input", &choices, &self.draft.midi.input) {
                    let draft = &mut self.draft.midi;
                    // A controller Halo knows brings its output and profile.
                    if let Some(input) = &input {
                        if draft.output.is_none() {
                            draft.output = midi::matching_output(input, live.outputs);
                        }
                        if draft.profile.is_none() {
                            draft.profile = MidiSettings::builtin_for(input).map(|p| p.id.clone());
                        }
                    }
                    draft.input = input;
                    self.learning = None;
                }
            });
            row(ui, "OUTPUT", Some("Where the LEDs and meters go."), |ui| {
                let choices = port_choices(live.outputs, self.saved.midi.output.as_deref());
                if let Some(output) = dropdown(ui, "midi_output", &choices, &self.draft.midi.output)
                {
                    self.draft.midi.output = output;
                }
            });
            let note = match self.draft.midi.profile() {
                Some(p) if p.is_builtin() => "Built-in. Duplicate it to change the mapping.",
                Some(_) => "Your own: learn each control below.",
                None => "Controls do nothing until a profile is chosen.",
            };
            row(ui, "PROFILE", Some(note), |ui| {
                let draft = &self.draft.midi;
                let mut choices = vec![("NONE".to_string(), None)];
                choices.extend(
                    midi::builtins()
                        .iter()
                        .chain(&draft.user_profiles)
                        .map(|p| (p.name.clone(), Some(p.id.clone()))),
                );
                if let Some(profile) = dropdown(ui, "midi_profile", &choices, &draft.profile) {
                    self.draft.midi.profile = profile;
                    self.learning = None;
                }
            });
            row(ui, "PROFILES", None, |ui| {
                let draft = &self.draft.midi;
                let current = draft.profile().cloned();
                let user = current.as_ref().is_some_and(|p| !p.is_builtin());
                let size = vec2(CONTROL_W / 3.0, CONTROL_H);
                let [new, duplicate, delete] = key::segmented(
                    ui,
                    [
                        Key::new("NEW").size(size),
                        Key::new("DUPLICATE").size(size).enabled(current.is_some()),
                        Key::new("DELETE").size(size).enabled(user),
                    ],
                );
                let draft = &mut self.draft.midi;
                let id = draft.new_profile_id();
                if new.clicked() {
                    draft.user_profiles.push(Profile {
                        id: id.clone(),
                        name: "New profile".into(),
                        port_match: None,
                        bindings: Vec::new(),
                        meters: Vec::new(),
                    });
                    draft.profile = Some(id);
                    self.learning = None;
                } else if duplicate.clicked()
                    && let Some(source) = current
                {
                    let name = format!("{} copy", source.name);
                    draft.user_profiles.push(Profile {
                        id: id.clone(),
                        name,
                        port_match: None,
                        ..source
                    });
                    draft.profile = Some(id);
                    self.learning = None;
                } else if delete.clicked()
                    && let Some(source) = current
                {
                    draft.user_profiles.retain(|p| p.id != source.id);
                    draft.profile = None;
                    self.learning = None;
                }
            });
            if let Some(profile) = self.draft.midi.user_profile_mut() {
                let valid = !profile.name.trim().is_empty();
                row(ui, "NAME", Some("What the profile is listed as."), |ui| {
                    text_field(ui, &mut profile.name, true, valid);
                });
            }
            let (value, note) = match &self.last {
                Some(m) => (m.label(), m.port.to_string()),
                None => ("—".to_string(), "Move a control on any device.".to_string()),
            };
            row(ui, "LAST MESSAGE", Some(&note), |ui| {
                wide_readout(ui, &value)
            });
        });

        let Some(profile) = self.draft.midi.profile().cloned() else {
            return;
        };
        let shift = profile.shift_key();
        for (heading, actions) in [
            ("DECK A", Action::deck(0).collect::<Vec<_>>()),
            ("DECK B", Action::deck(1).collect()),
            ("GLOBAL", Action::GLOBAL.to_vec()),
        ] {
            group(ui, heading, |ui| {
                for action in actions {
                    let learning = self.learning == Some(action);
                    let note = if learning {
                        "Move or press a control. Esc stops.".to_string()
                    } else {
                        match profile.binding(action) {
                            Some(b) if b.shifted => {
                                let shift = shift.map_or("SHIFT".into(), |k| k.label());
                                let key = b.key.label();
                                format!("{shift} + {key}")
                            }
                            Some(b) => b.key.label(),
                            None => "Not mapped.".to_string(),
                        }
                    };
                    row(ui, &action.label(), Some(&note), |ui| {
                        let editable = !profile.is_builtin();
                        let size = vec2(CONTROL_W / 2.0, CONTROL_H);
                        let [learn, clear] = key::segmented(
                            ui,
                            [
                                Key::new("LEARN")
                                    .size(size)
                                    .lit(learning.then_some(Lit::Accent))
                                    .pressed(learning)
                                    .enabled(editable),
                                Key::new("CLEAR")
                                    .size(size)
                                    .enabled(editable && profile.binding(action).is_some()),
                            ],
                        );
                        if learn.clicked() {
                            self.learning = (!learning).then_some(action);
                        }
                        if clear.clicked()
                            && let Some(profile) = self.draft.midi.user_profile_mut()
                        {
                            profile.clear(action);
                        }
                    });
                }
            });
        }
    }
}

/// NONE, the ports, and the saved one if it isn't connected.
fn port_choices(ports: &[String], saved: Option<&str>) -> Vec<(String, Option<String>)> {
    let mut choices = vec![("NONE".to_string(), None)];
    choices.extend(ports.iter().map(|p| (p.clone(), Some(p.clone()))));
    if let Some(saved) = saved
        && !ports.iter().any(|p| p == saved)
    {
        choices.push((format!("{saved} (not connected)"), Some(saved.to_string())));
    }
    choices
}

/// An input's activity light, lit while it's sending.
fn activity(ui: &mut Ui, on: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(CONTROL_W, CONTROL_H), Sense::hover());
    let led = pos2(rect.left() + FIELD_PAD, rect.center().y);
    theme::paint_led(ui.painter(), led, on, theme::ACCENT);
    if on {
        theme::paint_text(
            ui.painter(),
            pos2(led.x + FIELD_PAD, rect.center().y),
            Align2::LEFT_CENTER,
            "RECEIVING",
            theme::CAPTION,
            theme::TEXT_MUTED,
        );
    }
}

/// A read-only value across the whole control column, left-aligned.
fn wide_readout(ui: &mut Ui, value: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(CONTROL_W, CONTROL_H), Sense::hover());
    theme::paint_well(ui.painter(), rect, theme::RADIUS_CONTROL);
    let galley = truncated(
        ui.ctx(),
        value,
        theme::NUMERALS_MD,
        theme::TEXT_STRONG,
        CONTROL_W - 2.0 * FIELD_PAD,
    );
    ui.painter().galley(
        pos2(
            rect.left() + FIELD_PAD,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        theme::TEXT_STRONG,
    );
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
            cue_output: true,
            unicast: false,
            node_ip: "10.0.0.5".to_string(),
            midi: MidiSettings::default(),
        }
    }

    #[test]
    fn counts_changes_per_category() {
        let mut options = Options::new(saved(), Vec::new(), None, Some(48_000));
        assert_eq!(options.total_changes(), 0);
        options.draft.buffer_size = Some(512);
        options.draft.device_name = Some("Interface".to_string());
        assert_eq!(options.changes(Category::Audio), 2);
        options.draft.cue_output = false;
        assert_eq!(options.changes(Category::Audio), 3);
        // The node address only counts while it's the destination.
        options.draft.node_ip = "10.0.0.6".to_string();
        assert_eq!(options.changes(Category::Lighting), 0);
        options.draft.unicast = true;
        assert_eq!(options.changes(Category::Lighting), 2);
        assert_eq!(options.total_changes(), 5);
        // Back to the saved values: nothing to save.
        options.draft = saved();
        assert_eq!(options.total_changes(), 0);
    }

    #[test]
    fn headphone_cue_follows_the_draft_device() {
        let devices = vec![
            OutputDevice {
                name: "Speakers".to_string(),
                channels: 2,
            },
            OutputDevice {
                name: "Z1".to_string(),
                channels: 4,
            },
        ];
        let mut options = Options::new(saved(), devices, Some("Speakers".to_string()), None);
        // No device chosen: the system default's outputs count.
        assert!(!options.draft_device().unwrap().can_cue());
        options.draft.device_name = Some("Z1".to_string());
        assert!(options.draft_device().unwrap().can_cue());
        options.draft.device_name = Some("Unplugged".to_string());
        assert_eq!(options.draft_device(), None);
    }

    fn msg(port: &str, key: midi::MsgKey, value: f32) -> MidiMsg {
        MidiMsg {
            port: port.into(),
            key,
            value,
            raw: 0,
        }
    }

    #[test]
    fn learns_into_a_user_profile_and_counts_it() {
        use crate::midi::DeckControl;
        use crate::z1;
        let mut options = Options::new(saved(), Vec::new(), None, None);
        let draft = &mut options.draft.midi;
        draft.input = Some(z1::PORT.into());
        draft.profile = Some("builtin:traktor-kontrol-z1".into());
        assert_eq!(options.changes(Category::Midi), 2);

        // A copy of the built-in, with FX ON A's plain layer moved to HOT
        // CUE 1, learned with MODE held.
        let mut copy = options.draft.midi.profile().unwrap().clone();
        copy.id = "user:1".into();
        options.draft.midi.user_profiles.push(copy);
        options.draft.midi.profile = Some("user:1".into());
        let cue = Action::Deck(0, DeckControl::HotCue(0));
        options.learning = Some(cue);
        let (mode, fx) = (z1::key(z1::MODE), z1::key(z1::FX_A));
        options.take_messages(
            &[
                msg("Elsewhere", fx, 1.0),
                msg(z1::PORT, mode, 1.0),
                msg(z1::PORT, fx, 0.0),
                msg(z1::PORT, fx, 1.0),
            ],
            &[],
        );
        assert!(!options.learning(), "learned");
        let profile = options.draft.midi.profile().unwrap();
        assert_eq!(profile.resolve(fx, true), Some(cue));
        assert_eq!(
            profile.resolve(fx, false),
            Some(Action::Deck(0, DeckControl::FilterToggle)),
            "the plain layer is untouched"
        );
        assert_eq!(options.last.as_ref().map(|m| m.key), Some(fx));
        // Input, profile, and the new profile.
        assert_eq!(options.changes(Category::Midi), 3);
        options.draft = saved();
        assert_eq!(options.total_changes(), 0);
    }

    #[test]
    fn a_unicast_draft_needs_a_valid_address() {
        let mut options = Options::new(saved(), Vec::new(), None, None);
        options.draft.node_ip = "not an address".to_string();
        assert!(options.node_ip_valid(), "broadcast ignores the address");
        options.draft.unicast = true;
        assert!(!options.node_ip_valid());
        options.draft.node_ip = " 192.168.1.20 ".to_string();
        assert!(options.node_ip_valid());
    }
}
