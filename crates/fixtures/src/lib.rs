pub use fixture_library::{Channel, ChannelType, FixtureLibrary, FixtureProfile};
use serde::{Deserialize, Serialize};

mod fixture_library;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PanTiltLimits {
    pub pan_min: u8,
    pub pan_max: u8,
    pub tilt_min: u8,
    pub tilt_max: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fixture {
    pub id: usize,
    pub name: String,
    pub profile_id: String,
    #[serde(skip)]
    pub profile: FixtureProfile,
    #[serde(skip)] // Channels are copied from the profile during initialization
    pub channels: Vec<Channel>,
    pub universe: u8,
    pub start_address: u16,
    #[serde(default)]
    pub pan_tilt_limits: Option<PanTiltLimits>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum FixtureType {
    #[default]
    MovingHead,
    PAR,
    Wash,
    Beam,
    LEDBar,
    Pinspot,
    Smoke,
    PixelBar,
}

impl Fixture {
    pub fn new(
        id: usize,
        name: &str,
        profile: FixtureProfile,
        channels: Vec<Channel>,
        universe: u8,
        start_address: u16,
    ) -> Self {
        Fixture {
            id,
            name: name.to_string(),
            profile_id: profile.id.clone(),
            profile: profile.clone(),
            channels,
            universe,
            start_address,
            pan_tilt_limits: None,
        }
    }

    pub fn set_channel_value(&mut self, channel_type: &ChannelType, value: u8) {
        if let Some(channel) = self
            .channels
            .iter_mut()
            .find(|c| c.channel_type == *channel_type)
        {
            // Apply pan/tilt limits if they exist
            let clamped_value = if let Some(limits) = &self.pan_tilt_limits {
                match channel_type {
                    ChannelType::Pan => value.clamp(limits.pan_min, limits.pan_max),
                    ChannelType::Tilt => value.clamp(limits.tilt_min, limits.tilt_max),
                    _ => value,
                }
            } else {
                value
            };

            channel.value = clamped_value;
        }
    }

    pub fn get_dmx_values(&self) -> Vec<u8> {
        let mut values = Vec::new();
        for channel in &self.channels {
            values.push(channel.value);
        }
        values
    }

    pub fn set_pan_tilt_limits(&mut self, limits: PanTiltLimits) {
        self.pan_tilt_limits = Some(limits);
    }

    pub fn clear_pan_tilt_limits(&mut self) {
        self.pan_tilt_limits = None;
    }

    pub fn get_pan_tilt_limits(&self) -> Option<&PanTiltLimits> {
        self.pan_tilt_limits.as_ref()
    }
}

#[macro_export]
macro_rules! channel_layout {
    ($(($name:expr, $type:expr)),* $(,)?) => {
        vec![
            $(
                Channel {
                    name: $name.to_string(),
                    channel_type: $type,
                    value: 0,
                },
            )*
        ]
    };
}
