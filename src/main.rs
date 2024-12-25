mod ableton_link;
mod artnet;
mod console;
mod cue;
mod effect;
mod fixture;
mod midi;
mod rhythm;

use clap::Parser;
use std::time::{Duration, Instant};

use cue::{Chase, ChaseStep, Cue, EffectDistribution, EffectMapping, StaticValue};
use effect::{Effect, EffectParams};
use rhythm::Interval;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Whether to enable MIDI support
    #[arg(short, long)]
    enable_midi: bool,
}

fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();

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
    ];

    // Create the console
    let mut console = console::LightingConsole::new(80.).unwrap();
    console.load_fixture_library();

    // patch fixtures
    let _ = console.patch_fixture("Left PAR", "shehds-led-spot-60w", 1, 1);
    let _ = console.patch_fixture("Right PAR", "shehds-led-spot-60w", 1, 9);
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

    // load cues
    console.set_cues(cues);

    console.add_midi_override(
        77,
        midi::MidiOverride {
            // Pad 1 is note 45 on MPK49
            static_values: vec![
                StaticValue {
                    fixture_name: "Left PAR".to_string(),
                    channel_name: "Dimmer".to_string(),
                    value: 255,
                },
                StaticValue {
                    fixture_name: "Right PAR".to_string(),
                    channel_name: "Dimmer".to_string(),
                    value: 255,
                },
                StaticValue {
                    fixture_name: "Left PAR".to_string(),
                    channel_name: "Strobe".to_string(),
                    value: 20,
                },
                StaticValue {
                    fixture_name: "Right PAR".to_string(),
                    channel_name: "Strobe".to_string(),
                    value: 20,
                },
            ],
            velocity_sensitive: true,
        },
    );

    console.add_midi_override(
        60,
        midi::MidiOverride {
            static_values: vec![
                StaticValue {
                    fixture_name: "Smoke Machine".to_string(),
                    channel_name: "Blue".to_string(),
                    value: 255,
                },
                StaticValue {
                    fixture_name: "Smoke Machine".to_string(),
                    channel_name: "Strobe".to_string(),
                    value: 135,
                },
                StaticValue {
                    fixture_name: "Smoke Machine".to_string(),
                    channel_name: "Effect".to_string(),
                    value: 101,
                },
            ],
            velocity_sensitive: true,
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
