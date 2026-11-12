//! Fixture profiles and DMX patching, ported from halo-old's
//! `crates/fixtures`.
//!
//! A [`FixtureProfile`] is a pure template: the ordered channel layout a
//! fixture presents on the wire, in each of its DMX modes, and what its
//! colour and gobo wheels hold. Runtime channel values deliberately live
//! elsewhere (the resolve/output layer) — profiles and patches only
//! describe the rig. Profiles are hardcoded for now; loading from disk is
//! on the backlog.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::fixture::Fixture;

/// What a profile physically is, for grouping and defaulting in the
/// patch UI. Distinct from `fixture::FixtureKind`, which is the
/// programmer's grid grouping; the rig merge maps between them.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum FixtureType {
    #[default]
    MovingHead,
    Par,
    Wash,
    Beam,
    LedBar,
    Pinspot,
    Smoke,
    Pyro,
    PixelBar,
}

/// One DMX channel in a profile's layout (template only — no value).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    pub name: String,
    pub channel_type: ChannelType,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ChannelType {
    Dimmer,
    Color,
    Gobo,
    Red,
    Green,
    Blue,
    White,
    Amber,
    Uv,
    Strobe,
    Pan,
    /// The low byte of a 16-bit pan.
    PanFine,
    Tilt,
    /// The low byte of a 16-bit tilt.
    TiltFine,
    TiltSpeed,
    Beam,
    Focus,
    Zoom,
    Function,
    FunctionSpeed,
    /// Per-pixel color channels for pixel bars, 0-indexed.
    PixelRed(usize),
    PixelGreen(usize),
    PixelBlue(usize),
    Other(String),
}

/// One DMX mode of a fixture: the channels it presents, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FixtureMode {
    /// As the fixture's menu names it, e.g. "9CH".
    pub name: String,
    pub channel_layout: Vec<Channel>,
}

impl FixtureMode {
    /// Number of consecutive DMX addresses the fixture occupies.
    pub fn footprint(&self) -> usize {
        self.channel_layout.len()
    }

    /// Offset of the first channel of `channel_type` within the
    /// footprint, if the mode has one.
    pub fn channel_offset(&self, channel_type: &ChannelType) -> Option<usize> {
        self.channel_layout
            .iter()
            .position(|c| c.channel_type == *channel_type)
    }
}

/// One slot of a colour wheel: what it looks like, and the DMX value that
/// turns the wheel to it.
#[derive(Clone, Debug, PartialEq)]
pub struct WheelSlot {
    pub name: String,
    /// 0..=1 per channel.
    pub rgb: [f32; 3],
    pub dmx: u8,
}

#[derive(Clone, Debug, Default)]
pub struct FixtureProfile {
    pub id: String,
    pub fixture_type: FixtureType,
    pub manufacturer: String,
    pub model: String,
    /// At least one; the first is what a patch without a mode gets.
    pub modes: Vec<FixtureMode>,
    /// The colour wheel's slots, for a [`ChannelType::Color`] channel.
    /// Empty: the channel stays at 0.
    pub color_wheel: Vec<WheelSlot>,
    /// Each gobo's DMX value, in the programmer's GOBO 1… order. Empty:
    /// gobos spread evenly across the channel.
    pub gobo_wheel: Vec<u8>,
}

impl FixtureProfile {
    /// The mode called `name`, or the first for an empty or unknown name.
    pub fn mode(&self, name: &str) -> Option<&FixtureMode> {
        self.modes
            .iter()
            .find(|m| m.name == name)
            .or_else(|| self.modes.first())
    }
}

/// A profile's only mode, named after its channel count.
fn one(channel_layout: Vec<Channel>) -> Vec<FixtureMode> {
    vec![FixtureMode {
        name: format!("{}CH", channel_layout.len()),
        channel_layout,
    }]
}

impl std::fmt::Display for FixtureProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.manufacturer, self.model)
    }
}

/// Soft limits applied to pan/tilt values so a fixture can't be driven
/// into truss or walls. Applied by the output layer, not stored here.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PanTiltLimits {
    pub pan_min: u8,
    pub pan_max: u8,
    pub tilt_min: u8,
    pub tilt_max: u8,
}

impl PanTiltLimits {
    /// Clamp a value for the given channel; non-pan/tilt channels pass
    /// through untouched.
    pub fn clamp(&self, channel_type: &ChannelType, value: u8) -> u8 {
        match channel_type {
            ChannelType::Pan => value.clamp(self.pan_min, self.pan_max),
            ChannelType::Tilt => value.clamp(self.tilt_min, self.tilt_max),
            _ => value,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct FixtureLibrary {
    pub profiles: HashMap<String, FixtureProfile>,
}

fn ch(name: &str, channel_type: ChannelType) -> Channel {
    Channel {
        name: name.to_string(),
        channel_type,
    }
}

impl FixtureLibrary {
    pub fn new() -> Self {
        let mut profiles = HashMap::new();
        let mut add = |profile: FixtureProfile| {
            profiles.insert(profile.id.clone(), profile);
        };

        add(FixtureProfile {
            id: "shehds-rgbw-par".to_string(),
            fixture_type: FixtureType::Par,
            manufacturer: "Shehds".to_string(),
            model: "LED Flat PAR 12x3W RGBW".to_string(),
            modes: one(vec![
                ch("Dimmer", ChannelType::Dimmer),
                ch("Red", ChannelType::Red),
                ch("Green", ChannelType::Green),
                ch("Blue", ChannelType::Blue),
                ch("White", ChannelType::White),
                ch("Strobe", ChannelType::Strobe),
                ch("Program", ChannelType::Other("Program".to_string())),
                ch("Function", ChannelType::Other("Function".to_string())),
            ]),
            ..Default::default()
        });

        add(FixtureProfile {
            id: "shehds-led-spot-60w".to_string(),
            fixture_type: FixtureType::MovingHead,
            manufacturer: "Shehds".to_string(),
            model: "LED Spot 60W Lighting".to_string(),
            // 9CH first: patches from before modes load in it.
            modes: vec![
                FixtureMode {
                    name: "9CH".to_string(),
                    channel_layout: vec![
                        ch("Pan", ChannelType::Pan),
                        ch("Tilt", ChannelType::Tilt),
                        ch("Color", ChannelType::Color),
                        ch("Gobo", ChannelType::Gobo),
                        ch("Strobe", ChannelType::Strobe),
                        ch("Dimmer", ChannelType::Dimmer),
                        ch("Speed", ChannelType::Other("Speed".to_string())),
                        ch("Auto", ChannelType::Other("Auto".to_string())),
                        ch("Reset", ChannelType::Other("Reset".to_string())),
                    ],
                },
                FixtureMode {
                    name: "11CH".to_string(),
                    channel_layout: vec![
                        ch("Pan", ChannelType::Pan),
                        ch("Pan Fine", ChannelType::PanFine),
                        ch("Tilt", ChannelType::Tilt),
                        ch("Tilt Fine", ChannelType::TiltFine),
                        ch("Color", ChannelType::Color),
                        ch("Gobo", ChannelType::Gobo),
                        ch("Strobe", ChannelType::Strobe),
                        ch("Dimmer", ChannelType::Dimmer),
                        // 0 is the fastest move.
                        ch("Speed", ChannelType::Other("Speed".to_string())),
                        // 0–48: off.
                        ch("Auto", ChannelType::Other("Auto".to_string())),
                        ch("Reset", ChannelType::Other("Reset".to_string())),
                    ],
                },
            ],
            // From the manual's 11CH chart, at each slot's centre. The split
            // colours (80–139) and the auto cycles are left out, so a
            // preset always lands on a whole colour.
            color_wheel: [
                ("White", [1.0, 1.0, 1.0], 5),
                ("Red", [1.0, 0.0, 0.0], 15),
                ("Orange", [1.0, 0.5, 0.0], 25),
                ("Lemon Yellow", [1.0, 1.0, 0.0], 35),
                ("Green", [0.0, 1.0, 0.0], 45),
                ("Blue", [0.0, 0.0, 1.0], 55),
                ("Pink", [1.0, 0.4, 0.7], 65),
                ("Sky Blue", [0.4, 0.8, 1.0], 75),
            ]
            .into_iter()
            .map(|(name, rgb, dmx)| WheelSlot {
                name: name.to_string(),
                rgb,
                dmx,
            })
            .collect(),
            // Rose, Radial dots, Triangle, Hexagon, Swirl, Swirl, Jigsaw;
            // 0–7 is open. The jitter versions (72–127) are left out.
            gobo_wheel: vec![12, 20, 28, 36, 44, 52, 60],
        });

        add(FixtureProfile {
            id: "shehds-led-wash-7x18w-rgbwa-uv".to_string(),
            fixture_type: FixtureType::Wash,
            manufacturer: "Shehds".to_string(),
            model: "LED Wash 7x18W RGBWA+UV".to_string(),
            modes: one(vec![
                ch("Pan", ChannelType::Pan),
                ch("Tilt", ChannelType::Tilt),
                ch("Dimmer", ChannelType::Dimmer),
                ch("Red", ChannelType::Red),
                ch("Green", ChannelType::Green),
                ch("Blue", ChannelType::Blue),
                ch("White", ChannelType::White),
                ch("Amber", ChannelType::Amber),
                ch("UV", ChannelType::Uv),
                // TODO(halo-old): possibly XY speed — check the manual.
                ch("Function", ChannelType::Other("Function".to_string())),
            ]),
            ..Default::default()
        });

        add(FixtureProfile {
            id: "shehds-mini-led-pinspot-10w".to_string(),
            fixture_type: FixtureType::Pinspot,
            manufacturer: "Shehds".to_string(),
            model: "Mini LED Pinspot 10W".to_string(),
            modes: one(vec![
                ch("Dimmer", ChannelType::Dimmer),
                ch("Red", ChannelType::Red),
                ch("Green", ChannelType::Green),
                ch("Blue", ChannelType::Blue),
                ch("White", ChannelType::White),
                ch("Strobe", ChannelType::Strobe),
                // 0-50 none | 51-100 color select | 101-150 jump |
                // 151-200 gradient | 201-250 auto | 251-255 sound
                ch("Function", ChannelType::Other("Function".to_string())),
                // Slow → fast, paired with Function.
                ch("Speed", ChannelType::Other("FunctionSpeed".to_string())),
            ]),
            ..Default::default()
        });

        add(FixtureProfile {
            id: "dl-geyser-1000-led-smoke-machine-1000w-3x9w-rgb".to_string(),
            fixture_type: FixtureType::Smoke,
            manufacturer: "DL Geyser".to_string(),
            model: "1000 LED Smoke Machine".to_string(),
            modes: one(vec![
                ch("Smoke", ChannelType::Other("Smoke".to_string())),
                ch("Red", ChannelType::Red),
                ch("Green", ChannelType::Green),
                ch("Blue", ChannelType::Blue),
                ch("Strobe", ChannelType::Strobe),
                // LED effect: 0-50 off | 51-100 jump | 101-200 gradient |
                // 201-255 color strobe
                ch("Effect", ChannelType::Other("Function".to_string())),
                // Paired with Effect.
                ch("Speed", ChannelType::Other("FunctionSpeed".to_string())),
            ]),
            ..Default::default()
        });

        add(FixtureProfile {
            id: "shehds-led-bar-beam-8x12w".to_string(),
            fixture_type: FixtureType::Beam,
            manufacturer: "Shehds".to_string(),
            model: "LED Bar Beam 8x12W".to_string(),
            modes: one(vec![
                ch("Tilt", ChannelType::Tilt),
                ch("Tilt Speed", ChannelType::TiltSpeed),
                // 0-20 DMX 10ch | 21-70 transition | 71-120 gradual |
                // 121-170 clock | 171-220 run | 221-240 sound 1 |
                // 241-255 sound 2
                ch("Function", ChannelType::Function),
                ch("Speed", ChannelType::FunctionSpeed),
                ch("Dimmer", ChannelType::Dimmer),
                ch("Red", ChannelType::Red),
                ch("Green", ChannelType::Green),
                ch("Blue", ChannelType::Blue),
                ch("White", ChannelType::White),
            ]),
            ..Default::default()
        });

        // 6-channel mode (the 12-channel variant adds split RGB/White
        // shutter + FX banks; add it as a second profile when needed —
        // see https://personalities.avolites.com "LED RGBW 4in1 48
        // Partition Strobe Light").
        add(FixtureProfile {
            id: "hyulights-led-rgbw-4in1-48-partition-strobe".to_string(),
            fixture_type: FixtureType::LedBar,
            manufacturer: "Hyulights".to_string(),
            model: "200W LED RGBW 4in1 48 Partition Strobe Light".to_string(),
            modes: one(vec![
                ch("Dimmer", ChannelType::Dimmer),
                ch("Strobe", ChannelType::Strobe),
                ch("Red", ChannelType::Red),
                ch("Green", ChannelType::Green),
                ch("Blue", ChannelType::Blue),
                ch("White", ChannelType::White),
            ]),
            ..Default::default()
        });

        add(FixtureProfile {
            id: "hyulights-led-rgbw-par".to_string(),
            fixture_type: FixtureType::Par,
            manufacturer: "Hyulights".to_string(),
            model: "LED RGBW PAR Light".to_string(),
            modes: one(vec![
                ch("Dimmer", ChannelType::Dimmer),
                ch("Red", ChannelType::Red),
                ch("Green", ChannelType::Green),
                ch("Blue", ChannelType::Blue),
                ch("White", ChannelType::White),
                ch("Strobe", ChannelType::Strobe),
                ch("Function", ChannelType::Function),
                ch("Function Speed", ChannelType::FunctionSpeed),
            ]),
            ..Default::default()
        });

        for (id, model, pixels) in [
            ("generic-rgb-pixel-bar-30", "RGB Pixel Bar 30 Pixels", 30),
            ("generic-rgb-pixel-bar-60", "RGB Pixel Bar 60 Pixels", 60),
            ("generic-rgb-pixel-bar-144", "RGB Pixel Bar 144 Pixels", 144),
        ] {
            add(FixtureProfile {
                id: id.to_string(),
                fixture_type: FixtureType::PixelBar,
                manufacturer: "Generic".to_string(),
                model: model.to_string(),
                modes: one(pixel_bar_channels(pixels)),
                ..Default::default()
            });
        }
        add(FixtureProfile {
            id: "clen-led-pixel-bar-64".to_string(),
            fixture_type: FixtureType::PixelBar,
            manufacturer: "Clen".to_string(),
            model: "LED Pixel Bar 64 Pixels RGB".to_string(),
            modes: one(pixel_bar_channels(64)),
            ..Default::default()
        });

        // Not from halo-old: the default rig carries pyro units and needs
        // a profile for them. Standard 2ch DMX igniter convention —
        // Safety must be held high before Fire triggers.
        add(FixtureProfile {
            id: "generic-pyro-igniter".to_string(),
            fixture_type: FixtureType::Pyro,
            manufacturer: "Generic".to_string(),
            model: "DMX Pyro Igniter 2ch".to_string(),
            modes: one(vec![
                ch("Safety", ChannelType::Other("Safety".to_string())),
                ch("Fire", ChannelType::Other("Fire".to_string())),
            ]),
            ..Default::default()
        });

        FixtureLibrary { profiles }
    }

    pub fn get(&self, profile_id: &str) -> Option<&FixtureProfile> {
        self.profiles.get(profile_id)
    }

    /// A patched fixture's profile and the mode it's patched in.
    pub fn patched(&self, fixture: &Fixture) -> Option<(&FixtureProfile, &FixtureMode)> {
        let profile = self.get(&fixture.profile_id)?;
        Some((profile, profile.mode(&fixture.mode)?))
    }

    /// A patched fixture's mode.
    pub fn layout(&self, fixture: &Fixture) -> Option<&FixtureMode> {
        self.patched(fixture).map(|(_, mode)| mode)
    }
}

/// RGB-per-pixel layout for a pixel bar.
fn pixel_bar_channels(pixel_count: usize) -> Vec<Channel> {
    let mut channels = Vec::with_capacity(pixel_count * 3);
    for i in 0..pixel_count {
        channels.push(ch(
            &format!("Pixel {} Red", i + 1),
            ChannelType::PixelRed(i),
        ));
        channels.push(ch(
            &format!("Pixel {} Green", i + 1),
            ChannelType::PixelGreen(i),
        ));
        channels.push(ch(
            &format!("Pixel {} Blue", i + 1),
            ChannelType::PixelBlue(i),
        ));
    }
    channels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_profiles_are_well_formed() {
        let lib = FixtureLibrary::new();
        assert!(!lib.profiles.is_empty());
        for (key, profile) in &lib.profiles {
            assert_eq!(key, &profile.id, "map key must match profile id");
            assert!(!profile.modes.is_empty(), "{key}: no modes");
            for mode in &profile.modes {
                let name = &mode.name;
                assert!(mode.footprint() > 0, "{key} {name}: empty channel layout");
                assert!(
                    mode.footprint() <= 512,
                    "{key} {name}: footprint exceeds a universe"
                );
            }
        }
    }

    #[test]
    fn pixel_bar_layout_is_rgb_per_pixel() {
        let lib = FixtureLibrary::new();
        let bar = &lib.get("clen-led-pixel-bar-64").unwrap().modes[0];
        assert_eq!(bar.footprint(), 64 * 3);
        assert_eq!(
            bar.channel_offset(&ChannelType::PixelBlue(63)),
            Some(64 * 3 - 1)
        );
    }

    #[test]
    fn pan_tilt_limits_clamp_only_pan_tilt() {
        let limits = PanTiltLimits {
            pan_min: 10,
            pan_max: 200,
            tilt_min: 0,
            tilt_max: 128,
        };
        assert_eq!(limits.clamp(&ChannelType::Pan, 255), 200);
        assert_eq!(limits.clamp(&ChannelType::Tilt, 255), 128);
        assert_eq!(limits.clamp(&ChannelType::Dimmer, 255), 255);
    }
}
