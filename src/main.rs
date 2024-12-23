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
    let fixtures = fixture::create_fixtures();

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
                    fixture_name: "Moving Wash 1".to_string(),
                    channel_name: "Color".to_string(),
                    value: 127,
                },
                StaticValue {
                    fixture_name: "Moving Wash 1".to_string(),
                    channel_name: "Dimmer".to_string(),
                    value: 127,
                },
                StaticValue {
                    fixture_name: "Moving Wash 2".to_string(),
                    channel_name: "Color".to_string(),
                    value: 127,
                },
                StaticValue {
                    fixture_name: "Moving Wash 2".to_string(),
                    channel_name: "Dimmer".to_string(),
                    value: 127,
                },
                StaticValue {
                    fixture_name: "PAR Fixture 1".to_string(),
                    channel_name: "Red".to_string(),
                    value: 127,
                },
                StaticValue {
                    fixture_name: "PAR Fixture 2".to_string(),
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
                                    "Moving Wash 1".to_string(),
                                    "Moving Wash 2".to_string(),
                                ],
                                channel_types: vec![fixture::ChannelType::Tilt],
                                distribution: EffectDistribution::All,
                            }],
                            static_values: vec![
                                StaticValue {
                                    fixture_name: "Moving Wash 1".to_string(),
                                    channel_name: "Dimmer".to_string(),
                                    value: 255,
                                },
                                StaticValue {
                                    fixture_name: "Moving Wash 2".to_string(),
                                    channel_name: "Dimmer".to_string(),
                                    value: 255,
                                },
                            ],
                        },
                        ChaseStep {
                            //duration: 5.0,
                            duration: Duration::new(1, 0),
                            effect_mappings: vec![EffectMapping {
                                effect: effects[0].clone(), // Beat-Synced Sine Wave,
                                fixture_names: vec![
                                    "Moving Wash 1".to_string(),
                                    "Moving Wash 2".to_string(),
                                ],
                                channel_types: vec![fixture::ChannelType::Tilt],
                                distribution: EffectDistribution::All,
                            }],
                            static_values: vec![
                                StaticValue {
                                    fixture_name: "Moving Wash 1".to_string(),
                                    channel_name: "Dimmer".to_string(),
                                    value: 0,
                                },
                                StaticValue {
                                    fixture_name: "Moving Wash 2".to_string(),
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
                                    "PAR Fixture 1".to_string(),
                                    "PAR Fixture 2".to_string(),
                                ],
                                channel_types: vec![fixture::ChannelType::Dimmer],
                                distribution: EffectDistribution::All,
                            }],
                            static_values: vec![
                                StaticValue {
                                    fixture_name: "PAR Fixture 1".to_string(),
                                    channel_name: "Red".to_string(),
                                    value: 255,
                                },
                                StaticValue {
                                    fixture_name: "PAR Fixture 2".to_string(),
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
                                    "PAR Fixture 1".to_string(),
                                    "PAR Fixture 2".to_string(),
                                ],
                                channel_types: vec![fixture::ChannelType::Dimmer],
                                distribution: EffectDistribution::All,
                            }],
                            static_values: vec![
                                StaticValue {
                                    fixture_name: "PAR Fixture 1".to_string(),
                                    channel_name: "Red".to_string(),
                                    value: 0,
                                },
                                StaticValue {
                                    fixture_name: "PAR Fixture 2".to_string(),
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
            static_values: vec![
                // Set both PARs to full intensity on the Dimmer channel
                StaticValue {
                    fixture_name: "PAR Fixture 1".to_string(),
                    channel_name: "Dimmer".to_string(),
                    value: 255,
                },
                StaticValue {
                    fixture_name: "PAR Fixture 2".to_string(),
                    channel_name: "Dimmer".to_string(),
                    value: 255,
                },
                // Set both PARs to white
                StaticValue {
                    fixture_name: "PAR Fixture 1".to_string(),
                    channel_name: "White".to_string(),
                    value: 255,
                },
                StaticValue {
                    fixture_name: "PAR Fixture 2".to_string(),
                    channel_name: "White".to_string(),
                    value: 255,
                },
                // Set both spots to full intensity on the Dimmer channel
                StaticValue {
                    fixture_name: "Moving Head Spot 1".to_string(),
                    channel_name: "Dimmer".to_string(),
                    value: 255,
                },
                StaticValue {
                    fixture_name: "Moving Head Spot 2".to_string(),
                    channel_name: "Dimmer".to_string(),
                    value: 255,
                },
                // Set the left spot to blue and the right to purple
                StaticValue {
                    fixture_name: "Moving Head Spot 1".to_string(),
                    channel_name: "Color".to_string(),
                    value: 50,
                },
                StaticValue {
                    fixture_name: "Moving Head Spot 2".to_string(),
                    channel_name: "Color".to_string(),
                    value: 90,
                },
            ],
            chases: vec![
                Chase {
                    name: "PAR Alternating Chase".to_string(),
                    current_step: 0,
                    current_step_elapsed: 0.0,
                    accumulated_beats: 0.0,
                    last_step_change: Instant::now(),
                    // steps: vec![
                    //     ChaseStep {
                    //         duration: 5.0, // Duration of 2 beats
                    //         effect_mappings: vec![EffectMapping {
                    //             effect: effects[2].clone(),
                    //             fixture_names: vec!["PAR Fixture 1".to_string()],
                    //             channel_types: vec![fixture::ChannelType::Dimmer],
                    //             distribution: EffectDistribution::All,
                    //         }],
                    //         static_values: vec![], // Remove static values from the chase step
                    //     },
                    //     ChaseStep {
                    //         duration: 5.0, // Duration of 2 beats
                    //         effect_mappings: vec![EffectMapping {
                    //             effect: effects[2].clone(),
                    //             fixture_names: vec!["PAR Fixture 2".to_string()],
                    //             channel_types: vec![fixture::ChannelType::Dimmer],
                    //             distribution: EffectDistribution::All,
                    //         }],
                    //         static_values: vec![], // Remove static values from the chase step
                    //     },
                    // ],
                    steps: vec![
                        ChaseStep {
                            //duration: 8.0, // Duration of 1 beat
                            duration: Duration::from_secs(10),
                            effect_mappings: vec![EffectMapping {
                                effect: Effect {
                                    name: "Sawtooth Fade".to_string(),
                                    apply: effect::sawtooth_effect,
                                    min: 0,
                                    max: 255,
                                    params: EffectParams {
                                        interval: Interval::Bar,
                                        interval_ratio: 0.1,
                                        phase: 0.0,
                                    },
                                },
                                fixture_names: vec![
                                    "PAR Fixture 1".to_string(),
                                    "PAR Fixture 2".to_string(),
                                ],
                                channel_types: vec![fixture::ChannelType::Dimmer],
                                distribution: EffectDistribution::Step(1),
                            }],
                            static_values: vec![],
                        },
                        // ChaseStep {
                        //     //duration: 20.0, // Duration of 1 beat
                        //     duration: Duration::new(1, 0),
                        //     effect_mappings: vec![EffectMapping {
                        //         effect: Effect {
                        //             name: "Sawtooth Fade".to_string(),
                        //             apply: effect::sawtooth_effect,
                        //             min: 0,
                        //             max: 255,
                        //             params: EffectParams {
                        //                 interval: Interval::Beat,
                        //                 interval_ratio: 1.0,
                        //                 phase: 0.0,
                        //             },
                        //         },
                        //         fixture_names: vec!["PAR Fixture 2".to_string()],
                        //         channel_types: vec![fixture::ChannelType::Dimmer],
                        //         distribution: EffectDistribution::All,
                        //     }],
                        //     static_values: vec![],
                        // },
                    ],
                    loop_count: None, // Infinite loop
                },
                Chase {
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
                                fixture_names: vec!["Moving Head Spot 1".to_string()],
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
                                fixture_names: vec!["Moving Head Spot 1".to_string()],
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
                                fixture_names: vec!["Moving Head Spot 2".to_string()],
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
                                fixture_names: vec!["Moving Head Spot 2".to_string()],
                                channel_types: vec![fixture::ChannelType::Tilt],
                                distribution: EffectDistribution::Step(1),
                            },
                        ],
                        static_values: vec![],
                    }],
                    loop_count: None, // Infinite loop
                },
            ],
        },
    ];

    // Create the console
    let mut console = console::LightingConsole::new(80.).unwrap();
    console.set_fixtures(fixtures);
    console.set_cues(cues);

    console.add_midi_override(
        77,
        midi::MidiOverride {
            // Pad 1 is note 45 on MPK49
            static_values: vec![
                StaticValue {
                    fixture_name: "PAR Fixture 1".to_string(),
                    channel_name: "Dimmer".to_string(),
                    value: 255,
                },
                StaticValue {
                    fixture_name: "PAR Fixture 2".to_string(),
                    channel_name: "Dimmer".to_string(),
                    value: 255,
                },
                StaticValue {
                    fixture_name: "PAR Fixture 1".to_string(),
                    channel_name: "Strobe".to_string(),
                    value: 20,
                },
                StaticValue {
                    fixture_name: "PAR Fixture 2".to_string(),
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
