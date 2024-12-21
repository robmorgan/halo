mod ableton_link;
mod artnet;
mod color;
mod console;
mod cue;
mod effect;
mod fixture;
mod rhythm;

use std::time::{Duration, Instant};

use color::Color;
use cue::{Chase, ChaseStep, Cue, EffectDistribution, EffectMapping, StaticValue};
use effect::{Effect, EffectParams};
use fixture::{Channel, ChannelType, Fixture};
use rhythm::Interval;

fn main() -> Result<(), anyhow::Error> {
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

    let cues = vec![Cue {
        name: "Alternating PAR Chase".to_string(),
        fade_in_time: Duration::from_secs(0),
        fade_out_time: Duration::from_secs(0),
        duration: 60.0,
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
        ],
        chases: vec![
            Chase {
                name: "PAR Alternating Chase 1".to_string(),
                current_step: 0,
                current_step_elapsed: 0.0,
                accumulated_beats: 0.0,
                last_step_change: Instant::now(),
                steps: vec![ChaseStep {
                    //duration: 8.0, // Duration of 1 beat
                    duration: Duration::new(1, 500),
                    effect_mappings: vec![EffectMapping {
                        effect: Effect {
                            name: "Sine Fade".to_string(),
                            apply: effect::sine_effect,
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
                    static_values: vec![StaticValue::from_hex_color(
                        "PAR Fixture 1".to_string(),
                        //"#FFA500", // set to orange
                        //"#0000FF", // set to
                        //"#800080", // set to purple #800080
                        "#00FF00", // set to green #00FF00
                    )
                    .as_slice()]
                    .concat(),
                }],
                loop_count: None, // Infinite loop
            },
            Chase {
                name: "PAR Alternating Chase 2".to_string(),
                current_step: 0,
                current_step_elapsed: 0.0,
                accumulated_beats: 0.0,
                last_step_change: Instant::now(),
                steps: vec![ChaseStep {
                    //duration: 8.0, // Duration of 1 beat
                    duration: Duration::new(1, 500),
                    effect_mappings: vec![EffectMapping {
                        effect: Effect {
                            name: "Sine Fade".to_string(),
                            apply: effect::sine_effect,
                            min: 0,
                            max: 255,
                            params: EffectParams {
                                interval: Interval::Bar,
                                interval_ratio: 1.0,
                                phase: 64.0,
                            },
                        },
                        fixture_names: vec!["PAR Fixture 2".to_string()],
                        channel_types: vec![fixture::ChannelType::Dimmer],
                        distribution: EffectDistribution::All,
                    }],
                    static_values: vec![StaticValue::from_hex_color(
                        "PAR Fixture 2".to_string(),
                        "#FF0000",
                    )
                    .as_slice()]
                    .concat(),
                }],
                loop_count: None, // Infinite loop
            },
        ],
    }];

    // Create the console
    let mut console = console::LightingConsole::new(10.).unwrap();
    console.set_fixtures(fixtures);
    console.set_cues(cues);

    // run the show
    console.run();

    Ok(())
}
