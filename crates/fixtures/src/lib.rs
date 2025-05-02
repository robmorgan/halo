use serde::{Deserialize, Serialize};

pub use fixture_library::{Channel, ChannelType, FixtureLibrary, FixtureProfile};

mod fixture_library;

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
        }
    }

    pub fn set_channel_value(&mut self, channel_type: &ChannelType, value: u8) {
        if let Some(channel) = self
            .channels
            .iter_mut()
            .find(|c| c.channel_type == *channel_type)
        {
            channel.value = value;
        }
    }

    pub fn get_dmx_values(&self) -> Vec<u8> {
        let mut values = Vec::new();
        for channel in &self.channels {
            values.push(channel.value);
        }
        values
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
