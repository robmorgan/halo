//! Controllers: the MIDI ports (and the Z1 over HID), the messages they
//! send, and the profiles that map those messages onto Halo's controls and
//! Halo's state back onto the controller's LEDs and meters.
//!
//! Inputs arrive on CoreMIDI's thread (and the Z1's reader thread), which
//! only parse each message and queue it for the UI thread. The UI applies
//! them through the same paths the on-screen controls use, and sends the
//! feedback.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, mpsc};
use std::time::{Duration, Instant};

use eframe::egui;
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use serde::{Deserialize, Serialize};

use crate::z1;

/// The client name Halo's ports show under in other MIDI apps.
const CLIENT: &str = "Halo";
/// How often the port lists are re-read, so a controller plugged in (or
/// pulled) mid-set is picked up without a REFRESH.
const SCAN_EVERY: Duration = Duration::from_secs(2);
/// How long a port counts as active after its last message.
pub const ACTIVITY_HOLD: Duration = Duration::from_millis(150);
/// Round a bipolar knob's centre detent: this close to the middle is the
/// middle.
const DETENT: f32 = 0.03;

/// Control change, note, or one of the Z1's HID controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MsgKind {
    Cc,
    Note,
    Hid,
}

/// What a binding matches on: the kind, the channel (0-based) and the
/// number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MsgKey {
    pub kind: MsgKind,
    pub channel: u8,
    pub number: u8,
}

impl MsgKey {
    #[cfg(test)]
    pub const fn cc(channel: u8, number: u8) -> Self {
        Self {
            kind: MsgKind::Cc,
            channel,
            number,
        }
    }

    /// "CC 5 · CH 1", channels counted from 1 as controllers label them;
    /// a Z1 control by its name.
    pub fn label(&self) -> String {
        let (number, channel) = (self.number, self.channel + 1);
        match self.kind {
            MsgKind::Cc => format!("CC {number} · CH {channel}"),
            MsgKind::Note => format!("NOTE {number} · CH {channel}"),
            MsgKind::Hid => z1::name(number).to_string(),
        }
    }

    /// The three bytes that send `value` to this MIDI control.
    fn encode(&self, value: u8) -> Option<[u8; 3]> {
        let channel = self.channel & 0x0F;
        let value = value & 0x7F;
        match self.kind {
            MsgKind::Cc => Some([0xB0 | channel, self.number, value]),
            MsgKind::Note if value > 0 => Some([0x90 | channel, self.number, value]),
            MsgKind::Note => Some([0x80 | channel, self.number, 0]),
            MsgKind::Hid => None,
        }
    }
}

/// One incoming message, tagged with the port it came in on. `value` is
/// 0..1 whatever the resolution; `raw` is what the controller sent.
#[derive(Clone, Debug, PartialEq)]
pub struct MidiMsg {
    pub port: Arc<str>,
    pub key: MsgKey,
    pub value: f32,
    pub raw: u16,
}

impl MidiMsg {
    /// "CC 5 · CH 1 · 127", "GAIN A · 2048".
    pub fn label(&self) -> String {
        let (key, raw) = (self.key.label(), self.raw);
        format!("{key} · {raw}")
    }

    fn pressed(&self) -> bool {
        self.value > 0.0
    }
}

/// Control changes and notes; everything else (clock, SysEx, pitch bend…)
/// is dropped. A note-on at velocity 0 is a note-off.
pub fn parse(bytes: &[u8]) -> Option<(MsgKey, u8)> {
    let [status, number, value, ..] = *bytes else {
        return None;
    };
    let channel = status & 0x0F;
    let (kind, value) = match status & 0xF0 {
        0xB0 => (MsgKind::Cc, value),
        0x90 => (MsgKind::Note, value),
        0x80 => (MsgKind::Note, 0),
        _ => return None,
    };
    Some((
        MsgKey {
            kind,
            channel,
            number: number & 0x7F,
        },
        value & 0x7F,
    ))
}

/// A 7-bit value as 0..1, with 64 exactly in the middle.
pub fn from_7bit(value: u8) -> f32 {
    let v = f32::from(value.min(127));
    if v <= 64.0 {
        v / 128.0
    } else {
        0.5 + (v - 64.0) / 126.0
    }
}

/// A 0..1 value across `lo..=hi` with the middle exactly on `center`, so a
/// knob's detent is unity gain or no filter.
pub fn bipolar(value: f32, lo: f32, center: f32, hi: f32) -> f32 {
    let v = value.clamp(0.0, 1.0);
    let half = 0.5 - DETENT;
    if (v - 0.5).abs() < DETENT {
        center
    } else if v < 0.5 {
        lo + (center - lo) * (v / half).min(1.0)
    } else {
        center + (hi - center) * ((v - 0.5 - DETENT) / half).min(1.0)
    }
}

/// What a deck's controls do, for [`Action::Deck`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeckControl {
    Trim,
    EqHi,
    EqMid,
    EqLow,
    Filter,
    Fader,
    PlayPause,
    /// The CDJ cue: held previews, release returns.
    Cue,
    /// The filter out, and back in where it was.
    FilterToggle,
    HotCue(u8),
    AutoLoop,
    ExitLoop,
}

impl DeckControl {
    const ALL: [DeckControl; 19] = [
        DeckControl::Trim,
        DeckControl::EqHi,
        DeckControl::EqMid,
        DeckControl::EqLow,
        DeckControl::Filter,
        DeckControl::Fader,
        DeckControl::PlayPause,
        DeckControl::Cue,
        DeckControl::FilterToggle,
        DeckControl::AutoLoop,
        DeckControl::ExitLoop,
        DeckControl::HotCue(0),
        DeckControl::HotCue(1),
        DeckControl::HotCue(2),
        DeckControl::HotCue(3),
        DeckControl::HotCue(4),
        DeckControl::HotCue(5),
        DeckControl::HotCue(6),
        DeckControl::HotCue(7),
    ];

    fn label(self) -> String {
        match self {
            DeckControl::Trim => "TRIM".into(),
            DeckControl::EqHi => "HI".into(),
            DeckControl::EqMid => "MID".into(),
            DeckControl::EqLow => "LOW".into(),
            DeckControl::Filter => "FILTER".into(),
            DeckControl::Fader => "VOLUME".into(),
            DeckControl::PlayPause => "PLAY / PAUSE".into(),
            DeckControl::Cue => "CUE".into(),
            DeckControl::FilterToggle => "FILTER ON / OFF".into(),
            DeckControl::AutoLoop => "LOOP 4 BEATS".into(),
            DeckControl::ExitLoop => "EXIT LOOP".into(),
            DeckControl::HotCue(slot) => {
                let n = slot + 1;
                format!("HOT CUE {n}")
            }
        }
    }

    /// Knobs and faders follow the value; the rest act on press.
    fn is_button(self) -> bool {
        !matches!(
            self,
            DeckControl::Trim
                | DeckControl::EqHi
                | DeckControl::EqMid
                | DeckControl::EqLow
                | DeckControl::Filter
                | DeckControl::Fader
        )
    }
}

/// Everything a control can be bound to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    /// A deck's control: deck 0 is A, 1 is B.
    Deck(u8, DeckControl),
    AuditionVolume,
    /// Held, it switches every control to its shifted binding.
    Shift,
}

impl Action {
    /// One deck's actions, in the order the editor lists them.
    pub fn deck(deck: u8) -> impl Iterator<Item = Action> {
        DeckControl::ALL
            .into_iter()
            .map(move |control| Action::Deck(deck, control))
    }

    pub const GLOBAL: [Action; 2] = [Action::AuditionVolume, Action::Shift];

    pub fn label(self) -> String {
        match self {
            Action::Deck(_, control) => control.label(),
            Action::AuditionVolume => "AUDITION VOLUME".into(),
            Action::Shift => "SHIFT".into(),
        }
    }

    fn is_button(self) -> bool {
        match self {
            Action::Deck(_, control) => control.is_button(),
            Action::AuditionVolume => false,
            Action::Shift => true,
        }
    }
}

/// One control bound to one action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub key: MsgKey,
    /// Only while SHIFT is held.
    #[serde(default)]
    pub shifted: bool,
    pub action: Action,
}

/// A controller meter driven from a deck's level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meter {
    pub key: MsgKey,
    pub deck: u8,
}

/// The state behind a button's LED.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Led {
    Playing(u8),
    /// Paused on the cue point, as a CDJ's CUE lights.
    AtCue(u8),
    FilterOn(u8),
    Looping(u8),
    HotCueSet(u8, u8),
    Shift,
}

impl Led {
    fn of(action: Action) -> Option<Self> {
        Some(match action {
            Action::Deck(d, DeckControl::PlayPause) => Led::Playing(d),
            Action::Deck(d, DeckControl::Cue) => Led::AtCue(d),
            Action::Deck(d, DeckControl::FilterToggle) => Led::FilterOn(d),
            Action::Deck(d, DeckControl::AutoLoop) => Led::Looping(d),
            Action::Deck(d, DeckControl::HotCue(slot)) => Led::HotCueSet(d, slot),
            Action::Shift => Led::Shift,
            _ => return None,
        })
    }
}

/// One button's LED: what its plain and its shifted action show. `None`
/// where that layer has nothing with a state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LedState {
    pub key: MsgKey,
    pub plain: Option<bool>,
    pub shifted: Option<bool>,
}

/// Everything a profile shows on its controller this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Feedback {
    pub shift: bool,
    pub leds: Vec<LedState>,
    /// Each meter's deck peak, linear.
    pub meters: Vec<(MsgKey, f32)>,
}

/// A controller mapping. Built-ins live in code; user profiles are stored
/// in the library with the MIDI settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// A built-in is picked for an input whose name contains this.
    #[serde(default)]
    pub port_match: Option<String>,
    #[serde(default, deserialize_with = "known_bindings")]
    pub bindings: Vec<Binding>,
    #[serde(default)]
    pub meters: Vec<Meter>,
}

const BUILTIN_PREFIX: &str = "builtin:";

/// A profile's stored bindings, minus any for actions Halo no longer has,
/// so removing one never throws away the rest of the MIDI settings.
fn known_bindings<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Vec<Binding>, D::Error> {
    let stored = Vec::<serde_json::Value>::deserialize(de)?;
    Ok(stored
        .into_iter()
        .filter_map(|b| serde_json::from_value(b).ok())
        .collect())
}

impl Profile {
    pub fn is_builtin(&self) -> bool {
        self.id.starts_with(BUILTIN_PREFIX)
    }

    /// What `key` does, with SHIFT held or not. A control without a shifted
    /// binding keeps its plain one under SHIFT.
    pub fn resolve(&self, key: MsgKey, shift: bool) -> Option<Action> {
        let find = |shifted: bool| {
            self.bindings
                .iter()
                .find(|b| b.key == key && b.shifted == shifted)
                .map(|b| b.action)
        };
        if shift {
            find(true).or_else(|| find(false))
        } else {
            find(false)
        }
    }

    pub fn binding(&self, action: Action) -> Option<&Binding> {
        self.bindings.iter().find(|b| b.action == action)
    }

    /// The control SHIFT is on.
    pub fn shift_key(&self) -> Option<MsgKey> {
        self.binding(Action::Shift).map(|b| b.key)
    }

    /// Bind `action` to `key`, replacing its old control and whatever the
    /// control did before on that layer.
    pub fn learn(&mut self, action: Action, key: MsgKey, shifted: bool) {
        // SHIFT itself is never shifted.
        let shifted = shifted && action != Action::Shift;
        self.bindings
            .retain(|b| b.action != action && !(b.key == key && b.shifted == shifted));
        self.bindings.push(Binding {
            key,
            shifted,
            action,
        });
    }

    pub fn clear(&mut self, action: Action) {
        self.bindings.retain(|b| b.action != action);
    }

    /// The LEDs and meters for this frame: each bound button's own LED
    /// shows the state its actions leave behind.
    pub fn feedback(
        &self,
        shift: bool,
        led: impl Fn(Led) -> bool,
        peak: impl Fn(u8) -> f32,
    ) -> Feedback {
        let mut leds: Vec<LedState> = Vec::new();
        for b in &self.bindings {
            let Some(state) = Led::of(b.action).map(&led) else {
                continue;
            };
            let i = leds.iter().position(|l| l.key == b.key).unwrap_or_else(|| {
                leds.push(LedState {
                    key: b.key,
                    plain: None,
                    shifted: None,
                });
                leds.len() - 1
            });
            if b.shifted {
                leds[i].shifted = Some(state);
            } else {
                leds[i].plain = Some(state);
            }
        }
        Feedback {
            shift,
            leds,
            meters: self.meters.iter().map(|m| (m.key, peak(m.deck))).collect(),
        }
    }
}

/// Turns messages into actions for one profile: tracks SHIFT, and sends a
/// button's release to whatever its press did, even if SHIFT let go first.
#[derive(Default)]
pub struct Router {
    pub shift: bool,
    held: HashMap<MsgKey, Action>,
}

impl Router {
    pub fn route(&mut self, profile: &Profile, msg: &MidiMsg) -> Option<(Action, f32)> {
        let action = if msg.pressed() {
            let action = profile.resolve(msg.key, self.shift)?;
            if action.is_button() {
                self.held.insert(msg.key, action);
            }
            action
        } else {
            match self.held.remove(&msg.key) {
                Some(action) => action,
                None => profile.resolve(msg.key, self.shift)?,
            }
        };
        if action == Action::Shift {
            self.shift = msg.pressed();
        }
        Some((action, msg.value))
    }

    /// A new profile or input: nothing is held.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// The profiles that ship with Halo.
pub fn builtins() -> &'static [Profile] {
    static BUILTINS: LazyLock<Vec<Profile>> = LazyLock::new(|| vec![traktor_z1()]);
    &BUILTINS
}

/// Native Instruments Traktor Kontrol Z1, over HID as Mixxx maps it:
/// FX ON switches the filter, MODE + FX ON plays, MODE + headphone is the
/// cue. The headphone buttons alone do nothing, as Halo has no headphone
/// cue.
fn traktor_z1() -> Profile {
    let bind = |number, shifted, action| Binding {
        key: z1::key(number),
        shifted,
        action,
    };
    let mut bindings = Vec::new();
    for (deck, [gain, hi, mid, low, filter, volume, fx, headphone]) in [
        (
            0,
            [
                z1::GAIN_A,
                z1::HI_A,
                z1::MID_A,
                z1::LOW_A,
                z1::FILTER_A,
                z1::VOLUME_A,
                z1::FX_A,
                z1::HEADPHONE_A,
            ],
        ),
        (
            1,
            [
                z1::GAIN_B,
                z1::HI_B,
                z1::MID_B,
                z1::LOW_B,
                z1::FILTER_B,
                z1::VOLUME_B,
                z1::FX_B,
                z1::HEADPHONE_B,
            ],
        ),
    ] {
        let deck_action = |control| Action::Deck(deck, control);
        bindings.extend([
            bind(gain, false, deck_action(DeckControl::Trim)),
            bind(hi, false, deck_action(DeckControl::EqHi)),
            bind(mid, false, deck_action(DeckControl::EqMid)),
            bind(low, false, deck_action(DeckControl::EqLow)),
            bind(filter, false, deck_action(DeckControl::Filter)),
            bind(volume, false, deck_action(DeckControl::Fader)),
            bind(fx, false, deck_action(DeckControl::FilterToggle)),
            bind(fx, true, deck_action(DeckControl::PlayPause)),
            bind(headphone, true, deck_action(DeckControl::Cue)),
        ]);
    }
    bindings.extend([
        bind(z1::CUE_MIX, false, Action::AuditionVolume),
        bind(z1::MODE, false, Action::Shift),
    ]);
    Profile {
        id: format!("{BUILTIN_PREFIX}traktor-kontrol-z1"),
        name: "Traktor Kontrol Z1".into(),
        port_match: Some(z1::PORT.into()),
        bindings,
        meters: vec![
            Meter {
                key: z1::key(z1::METER_A),
                deck: 0,
            },
            Meter {
                key: z1::key(z1::METER_B),
                deck: 1,
            },
        ],
    }
}

/// Halo's MIDI settings: the ports, the profile and the user's own
/// profiles. Stored as JSON under the library's `midi` setting.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MidiSettings {
    pub input: Option<String>,
    pub output: Option<String>,
    /// The profile's id.
    pub profile: Option<String>,
    pub user_profiles: Vec<Profile>,
}

impl MidiSettings {
    pub fn find(&self, id: &str) -> Option<&Profile> {
        builtins()
            .iter()
            .chain(&self.user_profiles)
            .find(|p| p.id == id)
    }

    pub fn profile(&self) -> Option<&Profile> {
        self.profile.as_deref().and_then(|id| self.find(id))
    }

    /// The draft's user profile, to edit.
    pub fn user_profile_mut(&mut self) -> Option<&mut Profile> {
        let id = self.profile.as_deref()?;
        self.user_profiles.iter_mut().find(|p| p.id == id)
    }

    /// The built-in made for `input`, if there is one.
    pub fn builtin_for(input: &str) -> Option<&'static Profile> {
        builtins()
            .iter()
            .find(|p| p.port_match.as_deref().is_some_and(|m| input.contains(m)))
    }

    /// Settings for a first launch with a known controller connected: it,
    /// its output and the built-in made for it.
    pub fn suggested(inputs: &[String], outputs: &[String]) -> Option<Self> {
        inputs.iter().find_map(|input| {
            let profile = Self::builtin_for(input)?;
            Some(Self {
                input: Some(input.clone()),
                output: matching_output(input, outputs),
                profile: Some(profile.id.clone()),
                user_profiles: Vec::new(),
            })
        })
    }

    /// A fresh id for a user profile.
    pub fn new_profile_id(&self) -> String {
        (1..)
            .map(|n| format!("user:{n}"))
            .find(|id| self.find(id).is_none())
            .unwrap_or_default()
    }

    /// Whether either chosen port is the Z1 over HID.
    pub fn uses_z1(&self) -> bool {
        [&self.input, &self.output]
            .into_iter()
            .any(|p| p.as_deref() == Some(z1::PORT))
    }
}

/// The output that belongs to the same device as `input`: "Traktor Kontrol
/// Z1 Input" pairs with "Traktor Kontrol Z1 Output", the Z1 over HID with
/// itself.
pub fn matching_output(input: &str, outputs: &[String]) -> Option<String> {
    if outputs.iter().any(|o| o == input) {
        return Some(input.to_string());
    }
    let base = input
        .strip_suffix(" Input")
        .or_else(|| input.strip_suffix(" In"))
        .unwrap_or(input);
    outputs.iter().find(|o| o.starts_with(base)).cloned()
}

enum Output {
    Midi {
        name: String,
        connection: MidiOutputConnection,
        /// Values last sent, so feedback only sends changes.
        sent: HashMap<MsgKey, u8>,
    },
    Z1,
}

/// The ports and connections. Every MIDI input is open, so Options can
/// show activity and learn from any device; only the chosen one drives
/// Halo. The Z1 over HID is opened only while it's chosen, since opening
/// it takes it out of MIDI mode.
pub struct Midi {
    ctx: egui::Context,
    tx: mpsc::Sender<MidiMsg>,
    rx: mpsc::Receiver<MidiMsg>,
    /// Kept to re-read the port lists (opening a client per scan is slow).
    scan_in: Option<MidiInput>,
    scan_out: Option<MidiOutput>,
    /// The Z1's HID thread.
    #[cfg(target_os = "macos")]
    z1: Option<z1::Z1>,
    inputs: Vec<String>,
    outputs: Vec<String>,
    connections: Vec<MidiInputConnection<()>>,
    output: Option<Output>,
    scanned_at: Instant,
    last_seen: HashMap<Arc<str>, Instant>,
}

impl Midi {
    pub fn new(ctx: &egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut midi = Self {
            ctx: ctx.clone(),
            #[cfg(target_os = "macos")]
            z1: z1::Z1::spawn(tx.clone(), ctx),
            tx,
            rx,
            scan_in: MidiInput::new(CLIENT)
                .map_err(|e| log::warn!("midi input: {e}"))
                .ok(),
            scan_out: MidiOutput::new(CLIENT)
                .map_err(|e| log::warn!("midi output: {e}"))
                .ok(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            connections: Vec::new(),
            output: None,
            scanned_at: Instant::now(),
            last_seen: HashMap::new(),
        };
        midi.refresh(None);
        midi
    }

    /// Every input: the MIDI ports, and the Z1 over HID when it's plugged in.
    pub fn inputs(&self) -> &[String] {
        &self.inputs
    }

    pub fn outputs(&self) -> &[String] {
        &self.outputs
    }

    #[cfg(target_os = "macos")]
    fn z1_present(&self) -> bool {
        self.z1.as_ref().is_some_and(z1::Z1::present)
    }

    #[cfg(not(target_os = "macos"))]
    fn z1_present(&self) -> bool {
        false
    }

    fn port_names(&self) -> (Vec<String>, Vec<String>) {
        let mut inputs: Vec<String> = self.scan_in.as_ref().map_or_else(Vec::new, |m| {
            m.ports()
                .iter()
                .filter_map(|p| m.port_name(p).ok())
                .collect()
        });
        let mut outputs: Vec<String> = self.scan_out.as_ref().map_or_else(Vec::new, |m| {
            m.ports()
                .iter()
                .filter_map(|p| m.port_name(p).ok())
                .collect()
        });
        if self.z1_present() {
            inputs.push(z1::PORT.to_string());
            outputs.push(z1::PORT.to_string());
        }
        (inputs, outputs)
    }

    /// Re-read the ports and reopen every MIDI input and `output`. The Z1's
    /// thread looks after it.
    pub fn refresh(&mut self, output: Option<&str>) {
        self.scanned_at = Instant::now();
        let (inputs, outputs) = self.port_names();
        self.connections.clear();
        for name in inputs.iter().filter(|n| n.as_str() != z1::PORT) {
            if let Some(connection) = self.open_input(name) {
                self.connections.push(connection);
            }
        }
        log::info!("midi: {} inputs, {} outputs", inputs.len(), outputs.len());
        self.inputs = inputs;
        self.outputs = outputs;
        self.close_output();
        self.connect_output(output);
    }

    /// Refresh when the port lists change; cheap enough to call each frame.
    pub fn rescan(&mut self, output: Option<&str>) {
        if self.scanned_at.elapsed() < SCAN_EVERY {
            return;
        }
        self.scanned_at = Instant::now();
        let (inputs, outputs) = self.port_names();
        if inputs != self.inputs || outputs != self.outputs {
            self.refresh(output);
        }
    }

    fn open_input(&self, name: &str) -> Option<MidiInputConnection<()>> {
        let midi = MidiInput::new(CLIENT).ok()?;
        let port = midi
            .ports()
            .into_iter()
            .find(|p| midi.port_name(p).ok().as_deref() == Some(name))?;
        let (tx, ctx, port_name) = (self.tx.clone(), self.ctx.clone(), Arc::<str>::from(name));
        midi.connect(
            &port,
            "halo-in",
            move |_, bytes, _| {
                if let Some((key, raw)) = parse(bytes) {
                    let _ = tx.send(MidiMsg {
                        port: port_name.clone(),
                        key,
                        value: from_7bit(raw),
                        raw: u16::from(raw),
                    });
                    ctx.request_repaint();
                }
            },
            (),
        )
        .map_err(|e| log::warn!("midi: open {name}: {e}"))
        .ok()
    }

    /// Open the Z1 over HID only while it's chosen (in the settings, or in
    /// the Options draft): opening it ends its MIDI mode.
    pub fn want_z1(&mut self, wanted: bool) {
        #[cfg(target_os = "macos")]
        if let Some(z1) = &self.z1 {
            z1.set_wanted(wanted);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = wanted;
    }

    /// Feedback goes to `name`, the one before goes dark.
    pub fn connect_output(&mut self, name: Option<&str>) {
        let current = match &self.output {
            Some(Output::Midi { name, .. }) => Some(name.as_str()),
            Some(Output::Z1) => Some(z1::PORT),
            None => None,
        };
        if current == name {
            return;
        }
        self.close_output();
        let Some(name) = name else { return };
        if name == z1::PORT {
            self.output = Some(Output::Z1);
            return;
        }
        let Ok(midi) = MidiOutput::new(CLIENT) else {
            return;
        };
        let Some(port) = midi
            .ports()
            .into_iter()
            .find(|p| midi.port_name(p).ok().as_deref() == Some(name))
        else {
            return;
        };
        match midi.connect(&port, "halo-out") {
            Ok(connection) => {
                self.output = Some(Output::Midi {
                    name: name.to_string(),
                    connection,
                    sent: HashMap::new(),
                })
            }
            Err(e) => log::warn!("midi: open {name}: {e}"),
        }
    }

    /// This frame's messages, oldest first.
    pub fn drain(&mut self) -> Vec<MidiMsg> {
        let messages: Vec<MidiMsg> = self.rx.try_iter().collect();
        let now = Instant::now();
        for m in &messages {
            log::debug!("midi: {} {}", m.port, m.label());
            self.last_seen.insert(m.port.clone(), now);
        }
        messages
    }

    /// True for a little while after `port` sent something.
    pub fn active(&self, port: &str) -> bool {
        self.last_seen
            .get(port)
            .is_some_and(|t| t.elapsed() < ACTIVITY_HOLD)
    }

    /// Show `feedback` on the output, sending only what changed.
    pub fn show(&mut self, feedback: &Feedback) {
        match &mut self.output {
            Some(Output::Midi {
                connection, sent, ..
            }) => {
                let leds = feedback.leds.iter().map(|led| {
                    let on = if feedback.shift {
                        led.shifted.or(led.plain)
                    } else {
                        led.plain.or(led.shifted)
                    };
                    (led.key, if on == Some(true) { 127 } else { 0 })
                });
                let meters = feedback
                    .meters
                    .iter()
                    .map(|&(key, peak)| (key, meter_value(peak)));
                for (key, value) in leds.chain(meters) {
                    if sent.get(&key) == Some(&value) {
                        continue;
                    }
                    if let Some(bytes) = key.encode(value)
                        && connection.send(&bytes).is_ok()
                    {
                        sent.insert(key, value);
                    }
                }
            }
            #[cfg(target_os = "macos")]
            Some(Output::Z1) => {
                if let Some(z1) = &mut self.z1 {
                    z1.show(feedback);
                }
            }
            #[cfg(not(target_os = "macos"))]
            Some(Output::Z1) => {}
            None => {}
        }
    }

    /// Everything Halo lit, off again, and the output closed.
    fn close_output(&mut self) {
        if let Some(Output::Midi {
            connection, sent, ..
        }) = &mut self.output
        {
            for (key, value) in sent.iter() {
                if *value > 0
                    && let Some(bytes) = key.encode(0)
                {
                    let _ = connection.send(&bytes);
                }
            }
        }
        // The Z1 goes dark when it closes.
        self.output = None;
    }
}

impl Drop for Midi {
    fn drop(&mut self) {
        self.close_output();
    }
}

/// A deck's linear peak level as a 7-bit meter value: −36..0 dBFS across
/// 1..127, off below.
pub fn meter_value(peak: f32) -> u8 {
    const FLOOR_DB: f32 = -36.0;
    if peak <= 0.0 {
        return 0;
    }
    let db = 20.0 * peak.log10();
    if db < FLOOR_DB {
        return 0;
    }
    (1.0 + 126.0 * (1.0 - db / FLOOR_DB).min(1.0)).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(key: MsgKey, value: f32) -> MidiMsg {
        MidiMsg {
            port: Arc::from("test"),
            key,
            value,
            raw: 0,
        }
    }

    fn blank() -> Profile {
        Profile {
            id: "user:1".into(),
            name: "Test".into(),
            port_match: None,
            bindings: Vec::new(),
            meters: Vec::new(),
        }
    }

    #[test]
    fn parses_ccs_and_notes() {
        assert_eq!(parse(&[0xB2, 5, 64]), Some((MsgKey::cc(2, 5), 64)));
        let note = MsgKey {
            kind: MsgKind::Note,
            channel: 0,
            number: 36,
        };
        assert_eq!(parse(&[0x90, 36, 100]), Some((note, 100)));
        assert_eq!(parse(&[0x90, 36, 0]), Some((note, 0)), "velocity 0 is off");
        assert_eq!(parse(&[0x80, 36, 64]), Some((note, 0)));
        assert_eq!(parse(&[0xE0, 0, 64]), None, "pitch bend is dropped");
        assert_eq!(parse(&[0xF8]), None);
        assert_eq!(MsgKey::cc(2, 5).encode(127), Some([0xB2, 5, 127]));
        assert_eq!(note.encode(0), Some([0x80, 36, 0]));
        assert_eq!(z1::key(z1::MODE).encode(127), None);
    }

    #[test]
    fn the_detent_is_the_centre() {
        assert_eq!(from_7bit(0), 0.0);
        assert_eq!(from_7bit(64), 0.5);
        assert_eq!(from_7bit(127), 1.0);
        for (lo, center, hi) in [(0.0, 1.0, 2.0), (-1.0, 0.0, 1.0)] {
            assert_eq!(bipolar(0.5, lo, center, hi), center);
            assert_eq!(bipolar(0.0, lo, center, hi), lo);
            assert_eq!(bipolar(1.0, lo, center, hi), hi);
            // A Z1 knob resting a little off centre (1973 of 4095).
            assert_eq!(bipolar(1973.0 / 4095.0, lo, center, hi), center);
            assert_eq!(bipolar(from_7bit(63), lo, center, hi), center);
        }
        // Past the detent it moves off the centre at once.
        assert!(bipolar(0.6, 0.0, 1.0, 2.0) > 1.0);
        assert!(bipolar(0.4, 0.0, 1.0, 2.0) < 1.0);
    }

    #[test]
    fn shifted_bindings_win_while_shift_is_held() {
        let mut profile = blank();
        let button = MsgKey::cc(0, 5);
        let play = Action::Deck(0, DeckControl::PlayPause);
        let cue = Action::Deck(0, DeckControl::Cue);
        profile.learn(play, button, false);
        profile.learn(cue, button, true);
        assert_eq!(profile.resolve(button, false), Some(play));
        assert_eq!(profile.resolve(button, true), Some(cue));
        // Without a shifted binding, SHIFT leaves the plain one.
        let fader = MsgKey::cc(0, 6);
        profile.learn(Action::Deck(0, DeckControl::Fader), fader, false);
        assert_eq!(
            profile.resolve(fader, true),
            Some(Action::Deck(0, DeckControl::Fader))
        );
    }

    #[test]
    fn a_release_goes_where_its_press_went() {
        let z1 = traktor_z1();
        let mut router = Router::default();
        let (mode, headphone) = (z1::key(z1::MODE), z1::key(z1::HEADPHONE_A));
        let cue = Action::Deck(0, DeckControl::Cue);
        assert_eq!(
            router.route(&z1, &msg(headphone, 1.0)),
            None,
            "nothing plain"
        );
        router.route(&z1, &msg(mode, 1.0));
        assert!(router.shift);
        assert_eq!(router.route(&z1, &msg(headphone, 1.0)), Some((cue, 1.0)));
        // MODE lets go first; the release still ends the cue.
        router.route(&z1, &msg(mode, 0.0));
        assert!(!router.shift);
        assert_eq!(router.route(&z1, &msg(headphone, 0.0)), Some((cue, 0.0)));
        // Knobs follow their value, held or not.
        let trim = Action::Deck(0, DeckControl::Trim);
        assert_eq!(
            router.route(&z1, &msg(z1::key(z1::GAIN_A), 0.0)),
            Some((trim, 0.0))
        );
    }

    #[test]
    fn learning_replaces_the_old_binding() {
        let mut profile = traktor_z1();
        let toggle = Action::Deck(0, DeckControl::FilterToggle);
        let gain = z1::key(z1::GAIN_A);
        // The filter switch moves to the gain knob's control: trim loses
        // it, and FX ON's plain layer is free (its shifted PLAY stays).
        profile.learn(toggle, gain, false);
        assert_eq!(profile.resolve(gain, false), Some(toggle));
        assert!(
            profile
                .binding(Action::Deck(0, DeckControl::Trim))
                .is_none()
        );
        let fx = z1::key(z1::FX_A);
        assert_eq!(profile.resolve(fx, false), None);
        assert_eq!(
            profile.resolve(fx, true),
            Some(Action::Deck(0, DeckControl::PlayPause))
        );
        profile.clear(toggle);
        assert_eq!(profile.resolve(gain, false), None);
        // SHIFT is never learned shifted.
        profile.learn(Action::Shift, MsgKey::cc(0, 1), true);
        assert!(!profile.binding(Action::Shift).unwrap().shifted);
    }

    #[test]
    fn the_z1_binds_each_control_once() {
        let z1 = traktor_z1();
        assert!(z1.is_builtin());
        for (i, a) in z1.bindings.iter().enumerate() {
            for b in &z1.bindings[i + 1..] {
                assert!(
                    !(a.key == b.key && a.shifted == b.shifted),
                    "{a:?} and {b:?} share a control"
                );
                assert_ne!(a.action, b.action);
            }
        }
        assert_eq!(z1.shift_key(), Some(z1::key(z1::MODE)));
        // FX ON's blue LED is the filter, its red one the transport; the
        // headphone button shows the cue.
        let feedback = z1.feedback(
            false,
            |led| matches!(led, Led::FilterOn(0) | Led::AtCue(1)),
            f32::from,
        );
        let led = |number| {
            *feedback
                .leds
                .iter()
                .find(|l| l.key == z1::key(number))
                .unwrap()
        };
        assert_eq!(
            (led(z1::FX_A).plain, led(z1::FX_A).shifted),
            (Some(true), Some(false))
        );
        assert_eq!(
            (led(z1::HEADPHONE_B).plain, led(z1::HEADPHONE_B).shifted),
            (None, Some(true))
        );
        assert_eq!(
            feedback.meters,
            vec![(z1::key(z1::METER_A), 0.0), (z1::key(z1::METER_B), 1.0)]
        );
        assert_eq!(
            MidiSettings::builtin_for(z1::PORT).map(|p| &p.id),
            Some(&z1.id)
        );
    }

    #[test]
    fn bindings_to_actions_halo_dropped_are_skipped() {
        // A profile duplicated while the Z1's crossfader was still mapped.
        let json = r#"{
            "input": "Traktor Kontrol Z1 (HID)",
            "profile": "user:1",
            "user_profiles": [{
                "id": "user:1",
                "name": "Mine",
                "bindings": [
                    {"key": {"kind": "Hid", "channel": 0, "number": 13}, "action": "Crossfader"},
                    {"key": {"kind": "Hid", "channel": 0, "number": 14}, "action": "Shift"}
                ]
            }]
        }"#;
        let settings: MidiSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.input.as_deref(), Some(z1::PORT));
        let profile = settings.profile().unwrap();
        assert_eq!(profile.name, "Mine");
        assert_eq!(profile.bindings.len(), 1);
        assert_eq!(profile.shift_key(), Some(z1::key(z1::MODE)));
    }

    #[test]
    fn settings_round_trip_and_pair_ports() {
        let mut profile = traktor_z1();
        profile.id = "user:1".into();
        profile.port_match = None;
        let settings = MidiSettings {
            input: Some(z1::PORT.into()),
            output: Some(z1::PORT.into()),
            profile: Some("user:1".into()),
            user_profiles: vec![profile],
        };
        let json = serde_json::to_string(&settings).unwrap();
        let back: MidiSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, settings);
        assert!(back.uses_z1());
        assert_eq!(back.profile().map(|p| p.id.as_str()), Some("user:1"));
        assert_eq!(back.new_profile_id(), "user:2");
        assert_eq!(
            serde_json::from_str::<MidiSettings>("{}").unwrap(),
            MidiSettings::default()
        );

        let outputs = vec![
            "IAC Driver Bus 1".to_string(),
            "Traktor Kontrol Z1 Output".to_string(),
            z1::PORT.to_string(),
        ];
        assert_eq!(
            matching_output("Traktor Kontrol Z1 Input", &outputs).as_deref(),
            Some("Traktor Kontrol Z1 Output")
        );
        assert_eq!(
            matching_output(z1::PORT, &outputs).as_deref(),
            Some(z1::PORT)
        );
        let inputs = ["Traktor Kontrol Z1 Input".to_string(), z1::PORT.to_string()];
        let suggested = MidiSettings::suggested(&inputs, &outputs).unwrap();
        assert_eq!(suggested.input.as_deref(), Some(z1::PORT));
        assert_eq!(suggested.output.as_deref(), Some(z1::PORT));
        assert_eq!(
            suggested.profile.as_deref(),
            Some("builtin:traktor-kontrol-z1")
        );
    }

    #[test]
    fn meters_span_the_top_36_db() {
        assert_eq!(meter_value(0.0), 0);
        assert_eq!(meter_value(1.0), 127);
        assert_eq!(meter_value(0.01), 0, "−40 dB is below the floor");
        let half = meter_value(10f32.powf(-18.0 / 20.0));
        assert!((63..=65).contains(&half), "{half}");
    }
}
