pub struct Fixture {
    pub name: String,
    pub channels: Vec<Channel>,
    pub start_address: u16,
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
    pub is_16bit: bool,
    pub value: u16, // Using u16 to accommodate 16-bit channels
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
    pub fn new(name: &str, channels: Vec<Channel>, start_address: u16) -> Self {
        Fixture {
            name: name.to_string(),
            channels,
            start_address,
        }
    }

    pub fn set_channel_value(&mut self, channel_name: &str, value: u16) {
        if let Some(channel) = self.channels.iter_mut().find(|c| c.name == channel_name) {
            channel.value = value;
        }
    }

    pub fn get_dmx_values(&self) -> Vec<u8> {
        let mut values = Vec::new();
        for channel in &self.channels {
            if channel.is_16bit {
                values.push((channel.value >> 8) as u8);
                values.push((channel.value & 0xFF) as u8);
            } else {
                values.push(channel.value as u8);
            }
        }
        values
    }
}

pub fn create_fixtures() -> Vec<Fixture> {
    vec![
        Fixture::new(
            "PAR Fixture 1",
            vec![
                Channel {
                    name: "Dimmer".to_string(),
                    channel_type: ChannelType::Dimmer,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Red".to_string(),
                    channel_type: ChannelType::Red,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Green".to_string(),
                    channel_type: ChannelType::Green,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Blue".to_string(),
                    channel_type: ChannelType::Blue,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "White".to_string(),
                    channel_type: ChannelType::White,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Strobe".to_string(),
                    channel_type: ChannelType::Strobe,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Program".to_string(),
                    channel_type: ChannelType::Other("Program".to_string()),
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Function".to_string(),
                    channel_type: ChannelType::Other("Function".to_string()),
                    is_16bit: false,
                    value: 0,
                },
            ],
            1,
        ),
        Fixture::new(
            "PAR Fixture 2",
            vec![
                Channel {
                    name: "Dimmer".to_string(),
                    channel_type: ChannelType::Dimmer,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Red".to_string(),
                    channel_type: ChannelType::Red,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Green".to_string(),
                    channel_type: ChannelType::Green,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Blue".to_string(),
                    channel_type: ChannelType::Blue,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "White".to_string(),
                    channel_type: ChannelType::White,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Strobe".to_string(),
                    channel_type: ChannelType::Strobe,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Program".to_string(),
                    channel_type: ChannelType::Other("Program".to_string()),
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Function".to_string(),
                    channel_type: ChannelType::Other("Function".to_string()),
                    is_16bit: false,
                    value: 0,
                },
            ],
            9,
        ),
        Fixture::new(
            "Moving Wash 1",
            vec![
                Channel {
                    name: "Pan".to_string(),
                    channel_type: ChannelType::Pan,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Tilt".to_string(),
                    channel_type: ChannelType::Tilt,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Dimmer".to_string(),
                    channel_type: ChannelType::Dimmer,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Red".to_string(),
                    channel_type: ChannelType::Red,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Green".to_string(),
                    channel_type: ChannelType::Green,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Blue".to_string(),
                    channel_type: ChannelType::Blue,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "White".to_string(),
                    channel_type: ChannelType::White,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Amber".to_string(),
                    channel_type: ChannelType::Amber,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "UV".to_string(),
                    channel_type: ChannelType::UV,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Function".to_string(),
                    // TODO - I think this is XY speed?  Check the manual and update accordingly.
                    channel_type: ChannelType::Other("Function".to_string()),
                    is_16bit: false,
                    value: 0,
                },
            ],
            28,
        ),
        Fixture::new(
            "Moving Wash 2",
            vec![
                Channel {
                    name: "Pan".to_string(),
                    channel_type: ChannelType::Pan,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Tilt".to_string(),
                    channel_type: ChannelType::Tilt,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Dimmer".to_string(),
                    channel_type: ChannelType::Dimmer,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Red".to_string(),
                    channel_type: ChannelType::Red,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Green".to_string(),
                    channel_type: ChannelType::Green,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Blue".to_string(),
                    channel_type: ChannelType::Blue,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "White".to_string(),
                    channel_type: ChannelType::White,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Amber".to_string(),
                    channel_type: ChannelType::Amber,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "UV".to_string(),
                    channel_type: ChannelType::UV,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Function".to_string(),
                    // TODO - I think this is XY speed?  Check the manual and update accordingly.
                    channel_type: ChannelType::Other("Function".to_string()),
                    is_16bit: false,
                    value: 0,
                },
            ],
            39,
        ),
        Fixture::new(
            "Moving Head Spot 1",
            vec![
                Channel {
                    name: "Pan".to_string(),
                    channel_type: ChannelType::Pan,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Tilt".to_string(),
                    channel_type: ChannelType::Tilt,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Color".to_string(),
                    channel_type: ChannelType::Color,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Gobo".to_string(),
                    channel_type: ChannelType::Gobo,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Strobe".to_string(),
                    channel_type: ChannelType::Strobe,
                    is_16bit: false,
                    value: 0,
                },
                Channel {
                    name: "Dimmer".to_string(),
                    channel_type: ChannelType::Dimmer,
                    is_16bit: false,
                    value: 0,
                },
            ],
            55,
        ),
    ]
}
