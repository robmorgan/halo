mod ableton_link;
mod artnet;
mod console;
mod cue;
mod effect;
mod fixture;
mod rhythm;

use std::f64::consts::PI;
use std::io::{self, stdout, Read, Write};
use std::time::Duration;
use std::time::Instant;

use ableton_link::State;
use cue::{Chase, ChaseStep, Cue, EffectDistribution, EffectMapping, StaticValue};
use effect::{Effect, EffectParams};
use rhythm::Interval;

fn main() -> Result<(), anyhow::Error> {
    let fixtures = fixture::create_fixtures();

    let mut console = console::LightingConsole::new(128.).unwrap();
    console.set_fixtures(fixtures);

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
            max: 65535,
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
            max: 65535,
            params: effect::EffectParams {
                interval: rhythm::Interval::Bar,
                ..Default::default()
            },
        },
        effect::Effect {
            name: "Phrase-synced Sawtooth".to_string(),
            apply: effect::sawtooth_effect,
            min: 0,
            max: 65535,
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
                    fixture_name: "Moving Head 1".to_string(),
                    channel_name: "Color".to_string(),
                    value: 35000,
                },
                StaticValue {
                    fixture_name: "Moving Head 1".to_string(),
                    channel_name: "Dimmer".to_string(),
                    value: 35000,
                },
                StaticValue {
                    fixture_name: "Moving Head 2".to_string(),
                    channel_name: "Color".to_string(),
                    value: 35000,
                },
                StaticValue {
                    fixture_name: "Moving Head 2".to_string(),
                    channel_name: "Dimmer".to_string(),
                    value: 35000,
                },
                StaticValue {
                    fixture_name: "PAR Fixture 1".to_string(),
                    channel_name: "Red".to_string(),
                    value: 35000,
                },
                StaticValue {
                    fixture_name: "PAR Fixture 2".to_string(),
                    channel_name: "Red".to_string(),
                    value: 35000,
                },
            ],
            chases: vec![
                Chase {
                    name: "Moving Head Chase".to_string(),
                    steps: vec![
                        ChaseStep {
                            duration: 5.0,
                            effect_mappings: vec![EffectMapping {
                                effect: effects[0].clone(), // Beat-Synced Sine Wave,
                                fixture_names: vec![
                                    "Moving Head 1".to_string(),
                                    "Moving Head 2".to_string(),
                                ],
                                channel_types: vec![fixture::ChannelType::Tilt],
                                distribution: EffectDistribution::All,
                            }],
                            static_values: vec![
                                StaticValue {
                                    fixture_name: "Moving Head 1".to_string(),
                                    channel_name: "Dimmer".to_string(),
                                    value: 65535,
                                },
                                StaticValue {
                                    fixture_name: "Moving Head 2".to_string(),
                                    channel_name: "Dimmer".to_string(),
                                    value: 65535,
                                },
                            ],
                        },
                        ChaseStep {
                            duration: 5.0,
                            effect_mappings: vec![EffectMapping {
                                effect: effects[0].clone(), // Beat-Synced Sine Wave,
                                fixture_names: vec![
                                    "Moving Head 1".to_string(),
                                    "Moving Head 2".to_string(),
                                ],
                                channel_types: vec![fixture::ChannelType::Tilt],
                                distribution: EffectDistribution::All,
                            }],
                            static_values: vec![
                                StaticValue {
                                    fixture_name: "Moving Head 1".to_string(),
                                    channel_name: "Dimmer".to_string(),
                                    value: 0,
                                },
                                StaticValue {
                                    fixture_name: "Moving Head 2".to_string(),
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
                    steps: vec![
                        ChaseStep {
                            duration: 5.0, // Matches the total duration of the Moving Head Chase
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
                                    value: 65535,
                                },
                                StaticValue {
                                    fixture_name: "PAR Fixture 2".to_string(),
                                    channel_name: "Red".to_string(),
                                    value: 0,
                                },
                            ],
                        },
                        ChaseStep {
                            duration: 5.0, // Matches the total duration of the Moving Head Chase
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
                                    value: 65535,
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
            static_values: vec![
                // Set both PARs to full intensity on the Dimmer channel
                StaticValue {
                    fixture_name: "PAR Fixture 1".to_string(),
                    channel_name: "Dimmer".to_string(),
                    value: 65535,
                },
                StaticValue {
                    fixture_name: "PAR Fixture 2".to_string(),
                    channel_name: "Dimmer".to_string(),
                    value: 65535,
                },
                // Set both PARs to white
                StaticValue {
                    fixture_name: "PAR Fixture 1".to_string(),
                    channel_name: "White".to_string(),
                    value: 65535,
                },
                StaticValue {
                    fixture_name: "PAR Fixture 2".to_string(),
                    channel_name: "White".to_string(),
                    value: 65535,
                },
            ],
            chases: vec![Chase {
                name: "PAR Alternating Chase".to_string(),
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
                        duration: 1.0, // Duration of 1 beat
                        effect_mappings: vec![EffectMapping {
                            effect: Effect {
                                name: "Sawtooth Fade".to_string(),
                                apply: effect::sawtooth_effect,
                                min: 0,
                                max: 255,
                                params: EffectParams {
                                    interval: Interval::Beat,
                                    interval_ratio: 1.0,
                                    phase: 0.0,
                                },
                            },
                            fixture_names: vec!["PAR Fixture 1".to_string()],
                            channel_types: vec![fixture::ChannelType::Dimmer],
                            distribution: EffectDistribution::All,
                        }],
                        static_values: vec![],
                    },
                    ChaseStep {
                        duration: 1.0, // Duration of 1 beat
                        effect_mappings: vec![EffectMapping {
                            effect: Effect {
                                name: "Sawtooth Fade".to_string(),
                                apply: effect::sawtooth_effect,
                                min: 0,
                                max: 255,
                                params: EffectParams {
                                    interval: Interval::Beat,
                                    interval_ratio: 1.0,
                                    phase: 0.0,
                                },
                            },
                            fixture_names: vec!["PAR Fixture 2".to_string()],
                            channel_types: vec![fixture::ChannelType::Dimmer],
                            distribution: EffectDistribution::All,
                        }],
                        static_values: vec![],
                    },
                ],
                loop_count: None, // Infinite loop
            }],
        },
    ];

    // add cues
    console.set_cues(cues);

    // run the show
    console.run();

    Ok(())
}

//fixtures: &mut [fixture::Fixture], step: &ChaseStep, rhythm: &RhythmState

// fn apply_cue(fixtures: &mut [Fixture], cue: &Cue, beat_time: f64, tempo: f64) {
//     // Apply cue-level static values
//     for static_value in &cue.static_values {
//         if let Some(fixture) = fixtures
//             .iter_mut()
//             .find(|f| f.name == static_value.fixture_name)
//         {
//             fixture.set_channel_value(&static_value.channel_name, static_value.value);
//         }
//     }

//     // Apply chases
//     for chase in &cue.chases {
//         let chase_duration_beats: f64 = chase.steps.iter().map(|step| step.duration).sum();
//         let chase_beat_time = beat_time % chase_duration_beats;
//         let mut accumulated_beats = 0.0;

//         // TODO - will this only apply one step per loop?
//         for step in &chase.steps {
//             if chase_beat_time >= accumulated_beats
//                 && chase_beat_time < accumulated_beats + step.duration
//             {
//                 let step_beat_time = chase_beat_time - accumulated_beats;
//                 apply_chase_step(fixtures, step, beat_time, step_beat_time, tempo);
//                 break;
//             }
//             accumulated_beats += step.duration;
//         }
//     }
// }

//const SMOOTHING_FACTOR: f64 = 0.1; // Adjust this value to control transition speed (0.0 to 1.0)

fn beat_intensity(beat_time: f64) -> f64 {
    let beat_fraction = beat_time.fract();
    (1.0 - beat_fraction * 2.0).abs() // Creates a triangle wave that peaks on each beat
}
