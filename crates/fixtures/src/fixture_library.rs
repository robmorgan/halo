use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{channel_layout, FixtureType};

#[derive(Clone, Debug, Default)]
pub struct FixtureProfile {
    pub id: String,
    pub fixture_type: FixtureType,
    pub manufacturer: String,
    pub model: String,
    pub channel_layout: Vec<Channel>,
}

impl std::fmt::Display for FixtureProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.manufacturer, self.model)
    }
}

#[derive(Default)]
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
                id: "shehds-rgbw-par".to_string(),
                fixture_type: FixtureType::PAR,
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
                id: "shehds-led-spot-60w".to_string(),
                fixture_type: FixtureType::MovingHead,
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
                id: "shehds-led-wash-7x18w-rgbwa-uv".to_string(),
                fixture_type: FixtureType::Wash,
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
                        // TODO - I think this is XY speed? Check the manual and update accordingly.
                        channel_type: ChannelType::Other("Function".to_string()),
                        value: 0,
                    },
                ],
            },
        );

        profiles.insert(
            "shehds-mini-led-pinspot-10w".to_string(),
            FixtureProfile {
                id: "shehds-mini-led-pinspot-10w".to_string(),
                fixture_type: FixtureType::Pinspot,
                manufacturer: "Shehds".to_string(),
                model: "Mini LED Pinspot 10W".to_string(),
                channel_layout: channel_layout![
                    ("Dimmer", ChannelType::Dimmer),
                    ("Red", ChannelType::Red),
                    ("Green", ChannelType::Green),
                    ("Blue", ChannelType::Blue),
                    ("White", ChannelType::White),
                    ("Strobe", ChannelType::Strobe),
                    // 0-50: no effect
                    // 51-100: color selection mode
                    // 101-150: Jump mode
                    // 151-200: Gradient mode
                    // 201-250: Automatic mode
                    // 251-255: Voice control mode
                    ("Function", ChannelType::Other("Function".to_string())),
                    // From slow to fast
                    ("Speed", ChannelType::Other("FunctionSpeed".to_string())),
                ],
            },
        );

        profiles.insert(
            "dl-geyser-1000-led-smoke-machine-1000w-3x9w-rgb".to_string(),
            FixtureProfile {
                id: "dl-geyser-1000-led-smoke-machine-1000w-3x9w-rgb".to_string(),
                fixture_type: FixtureType::Smoke,
                manufacturer: "DL Geyser".to_string(),
                model: "1000 LED Smoke Machine".to_string(),
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
                    Channel {
                        // Works with the Effect channel
                        name: "Speed".to_string(),
                        channel_type: ChannelType::Other("FunctionSpeed".to_string()),
                        value: 0,
                    },
                ],
            },
        );

        profiles.insert(
            "shehds-led-bar-beam-8x12w".to_string(),
            FixtureProfile {
                id: "shehds-led-bar-beam-8x12w".to_string(),
                fixture_type: FixtureType::Beam,
                manufacturer: "Shehds".to_string(),
                model: "LED Bar Beam 8x12W".to_string(),
                channel_layout: channel_layout![
                    ("Tilt", ChannelType::Tilt),
                    ("Tilt Speed", ChannelType::TiltSpeed),
                    // 0-50: no effect
                    // 51-100: color selection mode
                    // 101-150: Jump mode
                    // 151-200: Gradient mode
                    // 201-250: Automatic mode
                    // 251-255: Voice control mode
                    // 0-20: DMX 10 Channel control.
                    // 21-70: Transition.
                    // 71-120: Gradual change.
                    // 121-170: Clock change.
                    // 171-220: Run change.
                    // 221-240: Sound 1 mode.
                    // 241-255: Sound 2 mode.
                    ("Function", ChannelType::Function),
                    // From slow to fast
                    ("Speed", ChannelType::FunctionSpeed),
                    ("Dimmer", ChannelType::Dimmer),
                    ("Red", ChannelType::Red),
                    ("Green", ChannelType::Green),
                    ("Blue", ChannelType::Blue),
                    ("White", ChannelType::White),
                ],
            },
        );

        // 1	Intensity	Master Dimmer	100%
        // 2	Intensity	RGB RGB Shutter	0%
        // 3	Effects	RGB RGB FX	No Effect
        // 4	Effects	RGB RGB FX Spd	Speed 0%
        // 5	Effects	RGB RGB FX Colour	Default
        // 9	Colour	RGB Red	100%	100%	0%
        // 10	Colour	RGB Green	100%	100%	0%
        // 11	Colour	RGB Blue	100%	0%	90%
        // 6	Intensity	White White Shutter	0%
        // 7	Effects	White White FX	No Effect
        // 8	Effects	White White FX Spd	50%
        // 12	Intensity	White Dimmer	100%

        // https://personalities.avolites.com/?mainPage=Main.asp&LightName=LED+RGBW+4in1+48+Partition+Strobe+Light&Manufacturer=Unknown
        // 12-channel variant
        // profiles.insert(
        //     "hyulights-led-rgbw-4in1-48-partition-strobe".to_string(),
        //     FixtureProfile {
        //         id: "hyulights-led-rgbw-4in1-48-partition-strobe".to_string(),
        //         fixture_type: FixtureType::LEDBar,
        //         manufacturer: "Hyulights".to_string(),
        //         model: "200W LED RGBW 4in1 48 Partition Strobe Light".to_string(),
        //         channel_layout: channel_layout![
        //             ("Dimmer", ChannelType::Dimmer),
        //             ("RGB Strobe", ChannelType::Other("RGBStrobe".to_string())),
        //             ("Effect FX", ChannelType::Other("Function".to_string())),
        //             (
        //                 "Effect FX Speed",
        //                 ChannelType::Other("FunctionSpeed".to_string())
        //             ),
        //             ("Color", ChannelType::Color),
        //             ("Strobe", ChannelType::Strobe),
        //             ("White FX", ChannelType::Other("WhiteFunction".to_string())),
        //             (
        //                 "White FX Speed",
        //                 ChannelType::Other("WhiteFunctionSpeed".to_string())
        //             ),
        //             ("Red", ChannelType::Red),
        //             ("Green", ChannelType::Green),
        //             ("Blue", ChannelType::Blue),
        //             ("White", ChannelType::White),
        //         ],
        //     },
        // );

        // 6-channel variant
        profiles.insert(
            "hyulights-led-rgbw-4in1-48-partition-strobe".to_string(),
            FixtureProfile {
                id: "hyulights-led-rgbw-4in1-48-partition-strobe".to_string(),
                fixture_type: FixtureType::LEDBar,
                manufacturer: "Hyulights".to_string(),
                model: "200W LED RGBW 4in1 48 Partition Strobe Light".to_string(),
                channel_layout: channel_layout![
                    ("Dimmer", ChannelType::Dimmer),
                    ("Strobe", ChannelType::Strobe),
                    ("Red", ChannelType::Red),
                    ("Green", ChannelType::Green),
                    ("Blue", ChannelType::Blue),
                    ("White", ChannelType::White),
                ],
            },
        );

        profiles.insert(
            "hyulights-led-rgbw-par".to_string(),
            FixtureProfile {
                id: "hyulights-led-rgbw-par".to_string(),
                fixture_type: FixtureType::PAR,
                manufacturer: "Hyulights".to_string(),
                model: "LED RGBW PAR Light".to_string(),
                channel_layout: channel_layout![
                    ("Dimmer", ChannelType::Dimmer),
                    ("Red", ChannelType::Red),
                    ("Green", ChannelType::Green),
                    ("Blue", ChannelType::Blue),
                    ("White", ChannelType::White),
                    ("Amber", ChannelType::Amber),
                    ("UV", ChannelType::UV),
                    ("Strobe", ChannelType::Strobe),
                    ("Function", ChannelType::Function),
                    ("Function Speed", ChannelType::FunctionSpeed),
                ],
            },
        );

        FixtureLibrary { profiles }
    }
}

#[derive(Clone, Debug)]
pub struct Channel {
    pub name: String,
    pub channel_type: ChannelType,
    pub value: u8,
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
    UV,
    Strobe,
    Pan,
    Tilt,
    TiltSpeed,
    Beam,
    Focus,
    Zoom,
    Function,
    FunctionSpeed,
    Other(String),
}

impl std::fmt::Display for ChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ChannelType::Dimmer => write!(f, "Dimmer"),
            ChannelType::Color => write!(f, "Color"),
            ChannelType::Gobo => write!(f, "Gobo"),
            ChannelType::Red => write!(f, "Red"),
            ChannelType::Green => write!(f, "Green"),
            ChannelType::Blue => write!(f, "Blue"),
            ChannelType::White => write!(f, "White"),
            ChannelType::Amber => write!(f, "Amber"),
            ChannelType::UV => write!(f, "UV"),
            ChannelType::Strobe => write!(f, "Strobe"),
            ChannelType::Pan => write!(f, "Pan"),
            ChannelType::Tilt => write!(f, "Tilt"),
            ChannelType::TiltSpeed => write!(f, "TiltSpeed"),
            ChannelType::Beam => write!(f, "Beam"),
            ChannelType::Focus => write!(f, "Focus"),
            ChannelType::Zoom => write!(f, "Zoom"),
            ChannelType::Function => write!(f, "Function"),
            ChannelType::FunctionSpeed => write!(f, "FunctionSpeed"),
            ChannelType::Other(s) => write!(f, "Other({})", s),
        }
    }
}
