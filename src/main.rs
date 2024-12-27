mod ableton_link;
mod artnet;
mod console;
mod cue;
mod effect;
mod fixture;
mod midi;
mod rhythm;

use clap::Parser;
use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

use console::NetworkConfig;
use cue::{Chase, ChaseStep, Cue, EffectDistribution, EffectMapping, StaticValue};
use effect::{Effect, EffectParams};
use midi::MidiAction;
use rhythm::Interval;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(name = "halo")]
#[command(about = "Halo lighting console")]
struct Args {
    /// Art-Net Source IP address
    #[arg(long, value_parser = parse_ip)]
    source_ip: IpAddr,

    /// Art-Net Destination IP address (optional - if not provided, broadcast mode will be used)
    #[arg(long, value_parser = parse_ip)]
    dest_ip: Option<IpAddr>,

    /// Art-Net port (default: 6454)
    #[arg(long, default_value = "6454")]
    artnet_port: u16,

    /// Force broadcast mode even if destination IP is provided
    #[arg(long, default_value = "false")]
    broadcast: bool,

    /// Whether to enable MIDI support
    #[arg(short, long)]
    enable_midi: bool,
}

fn parse_ip(s: &str) -> Result<IpAddr, String> {
    s.parse().map_err(|e| format!("Invalid IP address: {}", e))
}

fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();
    let network_config = NetworkConfig::new(
        args.source_ip,
        args.dest_ip,
        args.artnet_port,
        args.broadcast,
    );

    println!("Configuring Halo with Art-Net settings:");
    //    println!("Source IP: {}", network_config.source_ip);
    println!("Mode: {}", network_config.get_mode_string());
    println!("Destination: {}", network_config.get_destination());
    println!("Port: {}", network_config.port);

    // let fixture_groups = vec![
    //     fixture::Group {
    //         name: "Moving Heads".to_string(),
    //         fixture_names: vec!["Moving Head 1".to_string(), "Moving Head 2".to_string()],
    //     },
    //     fixture::Group {
    //         name: "PARs".to_string(),
    //         fixture_names: vec!["PAR Fixture 1".to_string(), "PAR Fixture 2".to_string()],
    //     },
    // ];

    let effects = vec![
        effect::Effect {
            name: "Beat-synced Sine".to_string(),
            apply: effect::sine_effect,
            min: 0,
            max: 255,
            params: effect::EffectParams {
                interval: rhythm::Interval::Beat,
                interval_ratio: 1.0, // Twice as fast
                phase: 0.25,         // Quarter phase offset
            },
        },
        effect::Effect {
            name: "Bar-synced Square".to_string(),
            apply: effect::square_effect,
            min: 0,
            max: 255,
            params: effect::EffectParams {
                interval: rhythm::Interval::Bar,
                ..Default::default()
            },
        },
        effect::Effect {
            name: "Phrase-synced Sawtooth".to_string(),
            apply: effect::sawtooth_effect,
            min: 0,
            max: 255,
            params: effect::EffectParams {
                interval: rhythm::Interval::Beat,
                interval_ratio: 1.0, // Twice as fast
                phase: 0.0,          // Quarter phase offset
            },
        },
    ];

    let cues = vec![
        Cue {
            name: "Complex Chase with Static Values".to_string(),
            duration: 10.0,
            static_values: vec![
                StaticValue {
                    fixture_name: "Left Wash".to_string(),
                    channel_name: "Color".to_string(),
                    value: 127,
                },
                StaticValue {
                    fixture_name: "Left Wash".to_string(),
                    channel_name: "Dimmer".to_string(),
                    value: 127,
                },
                StaticValue {
                    fixture_name: "Right Wash".to_string(),
                    channel_name: "Color".to_string(),
                    value: 127,
                },
                StaticValue {
                    fixture_name: "Right Wash".to_string(),
                    channel_name: "Dimmer".to_string(),
                    value: 127,
                },
                StaticValue {
                    fixture_name: "Left PAR".to_string(),
                    channel_name: "Red".to_string(),
                    value: 127,
                },
                StaticValue {
                    fixture_name: "Right PAR".to_string(),
                    channel_name: "Red".to_string(),
                    value: 127,
                },
            ],
            chases: vec![
                Chase {
                    name: "Moving Head Chase".to_string(),
                    current_step: 0,
                    current_step_elapsed: 0.0,
                    accumulated_beats: 0.0,
                    last_step_change: Instant::now(),
                    steps: vec![
                        ChaseStep {
                            //duration: 5.0,
                            duration: Duration::new(1, 0),
                            effect_mappings: vec![EffectMapping {
                                effect: effects[0].clone(), // Beat-Synced Sine Wave,
                                fixture_names: vec![
                                    "Left Wash".to_string(),
                                    "Right Wash".to_string(),
                                ],
                                channel_types: vec![fixture::ChannelType::Tilt],
                                distribution: EffectDistribution::All,
                            }],
                            static_values: vec![
                                StaticValue {
                                    fixture_name: "Left Wash".to_string(),
                                    channel_name: "Dimmer".to_string(),
                                    value: 255,
                                },
                                StaticValue {
                                    fixture_name: "Right Wash".to_string(),
                                    channel_name: "Dimmer".to_string(),
                                    value: 255,
                                },
                            ],
                        },
                        ChaseStep {
                            duration: Duration::new(1, 0),
                            effect_mappings: vec![EffectMapping {
                                effect: effects[0].clone(), // Beat-Synced Sine Wave,
                                fixture_names: vec![
                                    "Left Wash".to_string(),
                                    "Right Wash".to_string(),
                                ],
                                channel_types: vec![fixture::ChannelType::Tilt],
                                distribution: EffectDistribution::All,
                            }],
                            static_values: vec![
                                StaticValue {
                                    fixture_name: "Left Wash".to_string(),
                                    channel_name: "Dimmer".to_string(),
                                    value: 0,
                                },
                                StaticValue {
                                    fixture_name: "Right Wash".to_string(),
                                    channel_name: "Dimmer".to_string(),
                                    value: 0,
                                },
                            ],
                        },
                    ],
                    loop_count: None, // Infinite loop
                },
                Chase {
                    name: "PAR Chase".to_string(),
                    current_step: 0,
                    current_step_elapsed: 0.0,
                    accumulated_beats: 0.0,
                    last_step_change: Instant::now(),
                    steps: vec![
                        ChaseStep {
                            //duration: 5.0, // Matches the total duration of the Moving Head Chase
                            duration: Duration::new(1, 500),
                            effect_mappings: vec![EffectMapping {
                                effect: effects[1].clone(), // Beat-Synced Square Wave,
                                fixture_names: vec![
                                    "Left PAR".to_string(),
                                    "Right PAR".to_string(),
                                ],
                                channel_types: vec![fixture::ChannelType::Dimmer],
                                distribution: EffectDistribution::All,
                            }],
                            static_values: vec![
                                StaticValue {
                                    fixture_name: "Left PAR".to_string(),
                                    channel_name: "Red".to_string(),
                                    value: 255,
                                },
                                StaticValue {
                                    fixture_name: "Right PAR".to_string(),
                                    channel_name: "Red".to_string(),
                                    value: 0,
                                },
                            ],
                        },
                        ChaseStep {
                            //duration: 5.0, // Matches the total duration of the Moving Head Chase
                            duration: Duration::new(1, 500),
                            effect_mappings: vec![EffectMapping {
                                effect: effects[1].clone(), // Beat-Synced Square Wave,
                                fixture_names: vec![
                                    "Left PAR".to_string(),
                                    "Right PAR".to_string(),
                                ],
                                channel_types: vec![fixture::ChannelType::Dimmer],
                                distribution: EffectDistribution::All,
                            }],
                            static_values: vec![
                                StaticValue {
                                    fixture_name: "Left PAR".to_string(),
                                    channel_name: "Red".to_string(),
                                    value: 0,
                                },
                                StaticValue {
                                    fixture_name: "Right PAR".to_string(),
                                    channel_name: "Red".to_string(),
                                    value: 255,
                                },
                            ],
                        },
                    ],
                    loop_count: None, // Infinite loop
                },
            ],
        },
        Cue {
            name: "Alternating PAR Chase".to_string(),
            duration: 10.0,
            //duration: Duration::new(10, 0),
            static_values: static_values![
                // Set both PARs to full intensity on the Dimmer channel
                ("Left PAR", "Dimmer", 255),
                ("Right PAR", "Dimmer", 255),
                // Set both PARs to white
                ("Left PAR", "White", 255),
                ("Right PAR", "White", 255),
                // Set both spots to full intensity on the Dimmer channel
                ("Left Spot", "Dimmer", 255),
                ("Right Spot", "Dimmer", 255),
                // Set the left spot to blue and the right to purple
                ("Left Spot", "Color", 50),
                ("Right Spot", "Color", 90),
                // Set both washes to full intensity on the Dimmer channel
                //("Left Wash", "Dimmer", 255),
                //("Right Wash", "Dimmer", 255),
                // Set the left wash to blue and the right to purple
                ("Left Wash", "Red", 50),
                ("Right Wash", "Blue", 50),
            ],
            chases: vec![Chase {
                name: "Slow Dance Floor Sweep".to_string(),
                current_step: 0,
                current_step_elapsed: 0.0,
                accumulated_beats: 0.0,
                last_step_change: Instant::now(),
                steps: vec![ChaseStep {
                    duration: Duration::new(1, 0),
                    effect_mappings: vec![
                        EffectMapping {
                            effect: Effect {
                                name: "Pan Sweep".to_string(),
                                apply: effect::sine_effect,
                                min: 140,
                                max: 180,
                                params: EffectParams {
                                    interval: Interval::Phrase,
                                    interval_ratio: 2.0,
                                    phase: 0.0,
                                },
                            },
                            fixture_names: vec!["Left Spot".to_string(), "Left Wash".to_string()],
                            channel_types: vec![fixture::ChannelType::Pan],
                            distribution: EffectDistribution::Step(1),
                        },
                        EffectMapping {
                            effect: Effect {
                                name: "Tilt Movement".to_string(),
                                apply: effect::sine_effect,
                                min: 30,
                                max: 70,
                                params: EffectParams {
                                    interval: Interval::Phrase,
                                    interval_ratio: 2.0,
                                    phase: 0.0,
                                },
                            },
                            fixture_names: vec!["Left Spot".to_string(), "Left Wash".to_string()],
                            channel_types: vec![fixture::ChannelType::Tilt],
                            distribution: EffectDistribution::All,
                        },
                        EffectMapping {
                            effect: Effect {
                                name: "Pan Sweep".to_string(),
                                apply: effect::sine_effect,
                                min: 130,
                                max: 175,
                                params: EffectParams {
                                    interval: Interval::Phrase,
                                    interval_ratio: 1.0,
                                    phase: 0.0,
                                },
                            },
                            fixture_names: vec!["Right Spot".to_string()],
                            channel_types: vec![fixture::ChannelType::Pan],
                            distribution: EffectDistribution::Step(1),
                        },
                        EffectMapping {
                            effect: Effect {
                                name: "Tilt Movement".to_string(),
                                apply: effect::sine_effect,
                                min: 35,
                                max: 75,
                                params: EffectParams {
                                    interval: Interval::Phrase,
                                    interval_ratio: 1.0,
                                    phase: 180.0,
                                },
                            },
                            fixture_names: vec!["Right Spot".to_string()],
                            channel_types: vec![fixture::ChannelType::Tilt],
                            distribution: EffectDistribution::Step(1),
                        },
                        EffectMapping {
                            effect: Effect {
                                name: "Pan Sweep".to_string(),
                                apply: effect::sine_effect,
                                min: 130,
                                max: 175,
                                params: EffectParams {
                                    interval: Interval::Phrase,
                                    interval_ratio: 1.0,
                                    phase: 0.0,
                                },
                            },
                            fixture_names: vec!["Right Wash".to_string()],
                            channel_types: vec![fixture::ChannelType::Pan],
                            distribution: EffectDistribution::All,
                        },
                        EffectMapping {
                            effect: Effect {
                                name: "Tilt Movement".to_string(),
                                apply: effect::sine_effect,
                                min: 35,
                                max: 75,
                                params: EffectParams {
                                    interval: Interval::Phrase,
                                    interval_ratio: 1.0,
                                    phase: 180.0,
                                },
                            },
                            fixture_names: vec!["Right Wash".to_string()],
                            channel_types: vec![fixture::ChannelType::Tilt],
                            distribution: EffectDistribution::All,
                        },
                        EffectMapping {
                            effect: Effect {
                                name: "Dimmer Sidechain".to_string(),
                                apply: effect::sine_effect,
                                min: 0,
                                max: 255,
                                params: EffectParams {
                                    interval: Interval::Beat,
                                    interval_ratio: 1.0,
                                    phase: 0.0,
                                },
                            },
                            fixture_names: vec!["Left Wash".to_string(), "Right Wash".to_string()],
                            channel_types: vec![fixture::ChannelType::Dimmer],
                            distribution: EffectDistribution::Step(1),
                        },
                    ],
                    static_values: vec![
                        StaticValue {
                            fixture_name: "Left Spot".to_string(),
                            channel_name: "Dimmer".to_string(),
                            value: 125,
                        },
                        StaticValue {
                            fixture_name: "Right Spot".to_string(),
                            channel_name: "Dimmer".to_string(),
                            value: 255,
                        },
                    ],
                }],
                loop_count: None, // Infinite loop
            }],
        },
        Cue {
            name: "Pinspot Purple".to_string(),
            duration: 10.0,
            static_values: static_values![
                // Set the Pinspot to Deep Purple
                ("Pinspot", "Dimmer", 255),
                ("Pinspot", "Red", 147),
                ("Pinspot", "Blue", 211),
                ("Pinspot", "White", 20),
            ],
            chases: vec![],
        },
        Cue {
            name: "Pinspot Gradient".to_string(),
            duration: 10.0,
            static_values: static_values![
                ("Pinspot", "Dimmer", 255),
                ("Pinspot", "Function", 200),
                ("Pinspot", "Speed", 20),
            ],
            chases: vec![],
        },
    ];

    // Create the console
    let mut console = console::LightingConsole::new(80., network_config.clone()).unwrap();
    console.load_fixture_library();

    // patch fixtures
    let _ = console.patch_fixture("Left PAR", "shehds-rgbw-par", 1, 1);
    let _ = console.patch_fixture("Right PAR", "shehds-rgbw-par", 1, 9);
    let _ = console.patch_fixture("Left Spot", "shehds-led-spot-60w", 1, 18);
    let _ = console.patch_fixture("Right Spot", "shehds-led-spot-60w", 1, 28);
    let _ = console.patch_fixture("Left Wash", "shehds-led-wash-7x18w-rgbwa-uv", 1, 38);
    let _ = console.patch_fixture("Right Wash", "shehds-led-wash-7x18w-rgbwa-uv", 1, 48);
    let _ = console.patch_fixture(
        "Smoke Machine",
        "dl-geyser-1000-led-smoke-machine-1000w-3x9w-rgb",
        1,
        69,
    );
    let _ = console.patch_fixture("Pinspot", "shehds-mini-led-pinspot-10w", 1, 80);

    // load cues
    console.set_cues(cues);

    // Blue Strobe Fast
    console.add_midi_override(
        76,
        midi::MidiOverride {
            action: MidiAction::StaticValues(static_values![
                ("Smoke Machine", "Blue", 255),
                ("Smoke Machine", "Strobe", 255),
            ]),
        },
    );

    // Red Strobe Medium w/Half Smoke
    console.add_midi_override(
        77,
        midi::MidiOverride {
            action: MidiAction::StaticValues(static_values![
                ("Smoke Machine", "Smoke", 100),
                ("Smoke Machine", "Red", 255),
                ("Smoke Machine", "Strobe", 220),
            ]),
        },
    );

    // Blue Strobe Fast w/Full Smoke
    console.add_midi_override(
        78,
        midi::MidiOverride {
            action: MidiAction::StaticValues(static_values![
                ("Smoke Machine", "Smoke", 255),
                ("Smoke Machine", "Blue", 255),
                ("Smoke Machine", "Strobe", 255),
            ]),
        },
    );

    // Full Smoke
    console.add_midi_override(
        71,
        midi::MidiOverride {
            action: MidiAction::StaticValues(static_values![("Smoke Machine", "Smoke", 255),]),
        },
    );

    // Blue Pinspot
    // console.add_midi_override(
    //     72,
    //     midi::MidiOverride {
    //         action: MidiAction::StaticValues(static_values![
    //             ("Pinspot", "Dimmer", 255),
    //             ("Pinspot", "Blue", 255),
    //         ]),
    //     },
    // );

    // Blackout by setting all fixture dimmers to 0
    // console.add_midi_override(
    //     74,
    //     midi::MidiOverride {
    //         action: MidiAction::StaticValues(static_values![
    //             ("Left Spot", "Dimmer", 0),
    //             ("Right Spot", "Dimmer", 0),
    //             ("Left Wash", "Dimmer", 0),
    //             ("Right Wash", "Dimmer", 0),
    //             ("Pinspot", "Dimmer", 0),
    //             ("Smoke Machine", "Dimmer", 0),
    //         ]),
    //     },
    // );

    // Light Purple Pinspot
    // console.add_midi_override(
    //     67,
    //     midi::MidiOverride {
    //         action: MidiAction::StaticValues(static_values![
    //             ("Pinspot", "Dimmer", 255),
    //             ("Pinspot", "Red", 203),
    //             ("Pinspot", "Green", 160),
    //             ("Pinspot", "Blue", 255),
    //         ]),
    //     },
    // );

    //// Cue Overrides

    // Cue 5: Pinspot Purple
    console.add_midi_override(
        62,
        midi::MidiOverride {
            action: MidiAction::TriggerCue("Pinspot Purple".to_string()),
        },
    );

    // Cue 6: Pinspot Gradient
    console.add_midi_override(
        64,
        midi::MidiOverride {
            action: MidiAction::TriggerCue("Pinspot Gradient".to_string()),
        },
    );

    // Check if MIDI support is enabled
    if args.enable_midi {
        console.init_mpk49_midi()?;
    }

    // run the show
    console.run();

    Ok(())
}

#[macro_export]
macro_rules! static_values {
    ($(($fixture:expr, $channel:expr, $value:expr)),* $(,)?) => {
        vec![
            $(
                StaticValue {
                    fixture_name: $fixture.to_string(),
                    channel_name: $channel.to_string(),
                    value: $value,
                },
            )*
        ]
    };
}
