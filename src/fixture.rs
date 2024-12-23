use std::collections::HashMap;

pub struct Fixture {
    pub name: String,
    pub profile_name: String,
    pub channels: Vec<Channel>,
    pub universe: u8,
    pub start_address: u16,
}

#[derive(Clone, Debug)]
pub struct FixtureProfile {
    pub manufacturer: String,
    pub model: String,
    pub channel_layout: Vec<Channel>,
}

pub struct FixtureLibrary {
    pub profiles: HashMap<String, FixtureProfile>,
}

impl FixtureLibrary {
    pub fn new() -> Self {
        let mut profiles = HashMap::new();

        // Define all fixture profiles. Note in the future we'll load these from disk.
        profiles.insert(
            "shehds-rgbw-par".to_string(),
            FixtureProfile {
                manufacturer: "Shehds".to_string(),
                model: "LED Flat PAR 12x3W RGBW".to_string(),
                channel_layout: vec![
                    Channel {
                        name: "Dimmer".to_string(),
                        channel_type: ChannelType::Dimmer,
                        value: 0,
                    },
                    Channel {
                        name: "Red".to_string(),
                        channel_type: ChannelType::Red,
                        value: 0,
                    },
                    Channel {
                        name: "Green".to_string(),
                        channel_type: ChannelType::Green,
                        value: 0,
                    },
                    Channel {
                        name: "Blue".to_string(),
                        channel_type: ChannelType::Blue,
                        value: 0,
                    },
                    Channel {
                        name: "White".to_string(),
                        channel_type: ChannelType::White,
                        value: 0,
                    },
                    Channel {
                        name: "Strobe".to_string(),
                        channel_type: ChannelType::Strobe,
                        value: 0,
                    },
                    Channel {
                        name: "Program".to_string(),
                        channel_type: ChannelType::Other("Program".to_string()),
                        value: 0,
                    },
                    Channel {
                        name: "Function".to_string(),
                        channel_type: ChannelType::Other("Function".to_string()),
                        value: 0,
                    },
                ],
            },
        );

        profiles.insert(
            "shehds-led-spot-60w".to_string(),
            FixtureProfile {
                manufacturer: "Shehds".to_string(),
                model: "LED Spot 60W Lighting".to_string(),
                channel_layout: vec![
                    Channel {
                        name: "Pan".to_string(),
                        channel_type: ChannelType::Pan,
                        value: 0,
                    },
                    Channel {
                        name: "Tilt".to_string(),
                        channel_type: ChannelType::Tilt,
                        value: 0,
                    },
                    Channel {
                        name: "Color".to_string(),
                        channel_type: ChannelType::Color,
                        value: 0,
                    },
                    Channel {
                        name: "Gobo".to_string(),
                        channel_type: ChannelType::Gobo,
                        value: 0,
                    },
                    Channel {
                        name: "Strobe".to_string(),
                        channel_type: ChannelType::Strobe,
                        value: 0,
                    },
                    Channel {
                        name: "Dimmer".to_string(),
                        channel_type: ChannelType::Dimmer,
                        value: 0,
                    },
                    Channel {
                        name: "Speed".to_string(),
                        channel_type: ChannelType::Other("Speed".to_string()),
                        value: 0,
                    },
                    Channel {
                        name: "Auto".to_string(),
                        channel_type: ChannelType::Other("Auto".to_string()),
                        value: 0,
                    },
                    Channel {
                        name: "Reset".to_string(),
                        channel_type: ChannelType::Other("Reset".to_string()),
                        value: 0,
                    },
                ],
            },
        );

        profiles.insert(
            "shehds-led-wash-7x18w-rgbwa-uv".to_string(),
            FixtureProfile {
                manufacturer: "Shehds".to_string(),
                model: "LED Wash 7x18W RGBWA+UV".to_string(),
                channel_layout: vec![
                    Channel {
                        name: "Pan".to_string(),
                        channel_type: ChannelType::Pan,
                        value: 0,
                    },
                    Channel {
                        name: "Tilt".to_string(),
                        channel_type: ChannelType::Tilt,
                        value: 0,
                    },
                    Channel {
                        name: "Dimmer".to_string(),
                        channel_type: ChannelType::Dimmer,
                        value: 0,
                    },
                    Channel {
                        name: "Red".to_string(),
                        channel_type: ChannelType::Red,
                        value: 0,
                    },
                    Channel {
                        name: "Green".to_string(),
                        channel_type: ChannelType::Green,
                        value: 0,
                    },
                    Channel {
                        name: "Blue".to_string(),
                        channel_type: ChannelType::Blue,
                        value: 0,
                    },
                    Channel {
                        name: "White".to_string(),
                        channel_type: ChannelType::White,
                        value: 0,
                    },
                    Channel {
                        name: "Amber".to_string(),
                        channel_type: ChannelType::Amber,
                        value: 0,
                    },
                    Channel {
                        name: "UV".to_string(),
                        channel_type: ChannelType::UV,
                        value: 0,
                    },
                    Channel {
                        name: "Function".to_string(),
                        // TODO - I think this is XY speed?  Check the manual and update accordingly.
                        channel_type: ChannelType::Other("Function".to_string()),
                        value: 0,
                    },
                ],
            },
        );

        profiles.insert(
            "dl-geyser-1000-led-smoke-machine-1000w-3x9w-rgb".to_string(),
            FixtureProfile {
                manufacturer: "DL Geyser".to_string(),
                model: "DL Geyser 1000 LED Smoke Machine".to_string(),
                channel_layout: vec![
                    Channel {
                        name: "Smoke".to_string(),
                        channel_type: ChannelType::Other("Smoke".to_string()),
                        value: 0,
                    },
                    Channel {
                        name: "Red".to_string(),
                        channel_type: ChannelType::Red,
                        value: 0,
                    },
                    Channel {
                        name: "Green".to_string(),
                        channel_type: ChannelType::Green,
                        value: 0,
                    },
                    Channel {
                        name: "Blue".to_string(),
                        channel_type: ChannelType::Blue,
                        value: 0,
                    },
                    Channel {
                        name: "Strobe".to_string(),
                        channel_type: ChannelType::Strobe,
                        value: 0,
                    },
                    Channel {
                        name: "Effect".to_string(),
                        // LED Effect
                        // - 0-50: Off
                        // - 51-100: Jump
                        // - 101-200: Gradient
                        // - 201-255: Color Strobe
                        channel_type: ChannelType::Other("Function".to_string()),
                        value: 0,
                    },
                ],
            },
        );

        FixtureLibrary { profiles }
    }
}

#[derive(Clone, Debug)]
pub struct Group {
    name: String,
    fixture_names: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Channel {
    pub name: String,
    pub channel_type: ChannelType,
    pub value: u8,
}

#[derive(Clone, Debug)]
pub enum ChannelType {
    Dimmer,
    Color,
    Gobo,
    Red,
    Green,
    Blue,
    White,
    Amber,
    UV,
    Strobe,
    Pan,
    Tilt,
    TiltSpeed,
    Other(String),
}

impl Fixture {
    pub fn new(
        name: &str,
        profile_name: &str,
        channels: Vec<Channel>,
        universe: u8,
        start_address: u16,
    ) -> Self {
        Fixture {
            name: name.to_string(),
            profile_name: profile_name.to_string(),
            channels,
            universe,
            start_address,
        }
    }

    pub fn set_channel_value(&mut self, channel_name: &str, value: u8) {
        if let Some(channel) = self.channels.iter_mut().find(|c| c.name == channel_name) {
            channel.value = value;
        }
    }

    pub fn get_dmx_values(&self) -> Vec<u8> {
        let mut values = Vec::new();
        for channel in &self.channels {
            values.push(channel.value as u8);
        }
        values
    }
}
