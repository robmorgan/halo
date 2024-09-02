use artnet_protocol::{ArtCommand, Output, PortAddress};
use rusty_link::{AblLink, SessionState};
use std::collections::HashMap;
use std::error::Error;
use std::io::{self, stdout, Read, Write};
use std::net::SocketAddr;
use std::net::{ToSocketAddrs, UdpSocket};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use std::time::Instant;

const FIXTURES: usize = 4;
const CHANNELS_PER_FIXTURE: usize = 8; // SHEHDS PAR Fixtures
const TOTAL_CHANNELS: usize = FIXTURES * CHANNELS_PER_FIXTURE;
const TARGET_FREQUENCY: f64 = 44.0; // 44Hz DMX Spec
const TARGET_DELTA: f64 = 1.0 / TARGET_FREQUENCY;

struct Fixture {
    name: String,
    channels: Vec<Channel>,
    start_address: u16,
}

#[derive(Clone, Debug)]
struct FixtureGroup {
    name: String,
    fixture_names: Vec<String>,
}

#[derive(Clone, Debug)]
struct Channel {
    name: String,
    channel_type: ChannelType,
    is_16bit: bool,
    value: u16, // Using u16 to accommodate 16-bit channels
}

#[derive(Clone, Debug)]
enum ChannelType {
    Dimmer,
    Color,
    Gobo,
    Red,
    Green,
    Blue,
    White,
    Strobe,
    Pan,
    Tilt,
    TiltSpeed,
    Other(String),
}

#[derive(Clone, Debug)]
struct Effect {
    name: String,
    apply: fn(u16, f64, f64, f64) -> f64,
    min: u16,
    max: u16,
}

#[derive(Clone, Debug)]
enum EffectDistribution {
    All,
    Step(usize),
    Wave(f64), // Phase offset between fixtures
}

// TODO - one day we'll make this apply to multiple fixtures and channels
// TODO - this might be the case now
#[derive(Clone, Debug)]
struct EffectMapping {
    effect: Effect,
    fixture_names: Vec<String>,
    channel_types: Vec<ChannelType>,
    distribution: EffectDistribution,
}

#[derive(Clone, Debug)]
struct StaticValue {
    fixture_name: String,
    channel_name: String,
    value: u16,
}

#[derive(Clone, Debug)]
struct ChaseStep {
    duration: f64,
    effect_mappings: Vec<EffectMapping>,
    static_values: Vec<StaticValue>,
}

#[derive(Clone, Debug)]
struct Chase {
    name: String,
    steps: Vec<ChaseStep>,
    loop_count: Option<usize>, // None for infinite loop
}

struct Cue {
    name: String,
    duration: f64,
    static_values: Vec<StaticValue>,
    chases: Vec<Chase>,
}

// struct Cue {
//     name: String,
//     duration: f64,
//     effect_mappings: Vec<EffectMapping>,
//     static_values: Vec<StaticValue>,
// }

impl Fixture {
    fn new(name: &str, channels: Vec<Channel>, start_address: u16) -> Self {
        Fixture {
            name: name.to_string(),
            channels,
            start_address,
        }
    }

    fn set_channel_value(&mut self, channel_name: &str, value: u16) {
        if let Some(channel) = self.channels.iter_mut().find(|c| c.name == channel_name) {
            channel.value = value;
        }
    }

    fn get_dmx_values(&self) -> Vec<u8> {
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let link = AblLink::new(120.0);
    link.enable(false);

    let mut state = SessionState::new();
    link.capture_app_session_state(&mut state);
    link.enable(true);

    //let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
    let socket = UdpSocket::bind((String::from("0.0.0.0"), 6455))?;
    //let target_addr = "192.168.1.100:6454"; // Replace with your Art-Net node's IP and port

    let broadcast_addr = ("255.255.255.255", 6454)
        .to_socket_addrs()
        .unwrap()
        .next()
        .unwrap();
    socket.set_broadcast(true).unwrap();

    let mut fixtures = vec![
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
            "Moving Head 1",
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
            169,
        ),
        Fixture::new(
            "Moving Head 2",
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
            178,
        ),
    ];

    let fixture_groups = vec![
        FixtureGroup {
            name: "Moving Heads".to_string(),
            fixture_names: vec!["Moving Head 1".to_string(), "Moving Head 2".to_string()],
        },
        FixtureGroup {
            name: "PARs".to_string(),
            fixture_names: vec!["PAR Fixture 1".to_string(), "PAR Fixture 2".to_string()],
        },
    ];

    let effects = vec![
        Effect {
            name: "Sine Wave".to_string(),
            apply: sine_wave_effect,
            min: 0,
            max: 65535,
        },
        Effect {
            name: "Square Wave".to_string(),
            apply: square_wave_effect,
            min: 0,
            max: 65535,
        },
        Effect {
            name: "Sawtooth Wave".to_string(),
            apply: sawtooth_wave_effect,
            min: 0,
            max: 65535,
        },
    ];

    let cues = vec![Cue {
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
                fixture_name: "LED Bar 1".to_string(),
                channel_name: "Red".to_string(),
                value: 255,
            },
            StaticValue {
                fixture_name: "LED Bar 2".to_string(),
                channel_name: "Blue".to_string(),
                value: 255,
            },
        ],
        chases: vec![
            Chase {
                name: "Moving Head Chase".to_string(),
                steps: vec![
                    ChaseStep {
                        duration: 5.0,
                        effect_mappings: vec![EffectMapping {
                            effect: Effect {
                                name: "Tilt Down".to_string(),
                                apply: linear_effect,
                                min: 32768, // 50%
                                max: 65535, // 100%
                            },
                            fixture_names: vec![
                                "Moving Head 1".to_string(),
                                "Moving Head 2".to_string(),
                            ],
                            channel_types: vec![ChannelType::Tilt],
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
                            effect: Effect {
                                name: "Tilt Up".to_string(),
                                apply: linear_effect,
                                min: 32768, // 50%
                                max: 0,     // 0%
                            },
                            fixture_names: vec![
                                "Moving Head 1".to_string(),
                                "Moving Head 2".to_string(),
                            ],
                            channel_types: vec![ChannelType::Tilt],
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
                name: "LED Bar Chase".to_string(),
                steps: vec![ChaseStep {
                    duration: 10.0, // Matches the total duration of the Moving Head Chase
                    effect_mappings: vec![EffectMapping {
                        effect: Effect {
                            name: "Pan".to_string(),
                            apply: sine_wave_effect,
                            min: 0,
                            max: 65535,
                        },
                        fixture_names: vec!["LED Bar 1".to_string(), "LED Bar 2".to_string()],
                        channel_types: vec![ChannelType::Pan],
                        distribution: EffectDistribution::All,
                    }],
                    static_values: vec![
                        StaticValue {
                            fixture_name: "LED Bar 1".to_string(),
                            channel_name: "Dimmer".to_string(),
                            value: 65535,
                        },
                        StaticValue {
                            fixture_name: "LED Bar 2".to_string(),
                            channel_name: "Dimmer".to_string(),
                            value: 65535,
                        },
                    ],
                }],
                loop_count: None, // Infinite loop
            },
        ],
    }];

    // Keyboard input handling
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || loop {
        let mut buffer = [0; 1];
        if io::stdin().read_exact(&mut buffer).is_ok() {
            if buffer[0] == b'G' || buffer[0] == b'g' {
                tx.send(()).unwrap();
            }
        }
    });

    let mut current_cue = 0;
    let mut frames_sent = 0;
    let mut accumulated_time = 0.0;
    let mut cue_time = 0.0;
    let mut last_update = Instant::now();

    let mut cue_start_time = 0.0;
    let mut bpm = 0.0;
    let mut frames_sent = 0;
    let start_time = Instant::now();
    let mut accumulated_time = 0.0;
    let mut effect_time = 0.0;

    loop {
        let now = Instant::now();
        let delta = now.duration_since(last_update).as_secs_f64();
        last_update = now;

        accumulated_time += delta;
        cue_time += delta;

        link.capture_app_session_state(&mut state);
        let beat_time = state.beat_at_time(link.clock_micros(), 0.0);

        if cue_time >= cues[current_cue].duration {
            cue_time = 0.0; // Reset cue time but don't change the cue
        }

        if rx.try_recv().is_ok() {
            current_cue = (current_cue + 1) % cues.len();
            cue_time = 0.0;
            println!("Advanced to cue: {}", cues[current_cue].name);
        }

        // apply_cue(
        //     &mut fixtures,
        //     &cues[current_cue],
        //     accumulated_time,
        //     cue_time,
        //     delta,
        // );

        apply_cue(&mut fixtures, &cues[current_cue], accumulated_time);

        let dmx_data = generate_dmx_data(&fixtures);
        send_dmx_data(&socket, broadcast_addr, dmx_data)?;
        frames_sent += 1;

        if beat_time - cue_start_time >= cues[current_cue].duration {
            cue_start_time = beat_time;
        }

        // Display status information
        bpm = state.tempo();

        display_status(
            &link,
            bpm,
            frames_sent,
            &cues[current_cue].name,
            accumulated_time,
            cue_time,
        );

        let frame_time = now.elapsed().as_secs_f64();
        if frame_time < TARGET_DELTA {
            std::thread::sleep(Duration::from_secs_f64(TARGET_DELTA - frame_time));
        }
    }
}

// fn apply_cue(fixtures: &mut [Fixture], cue: &Cue, total_time: f64, cue_time: f64, delta: f64) {
//     for mapping in &cue.effect_mappings {
//         if let Some(fixture) = fixtures.iter_mut().find(|f| f.name == mapping.fixture_name) {
//             for channel in &mut fixture.channels {
//                 if channel.name.starts_with(&mapping.channel_pattern) {
//                     channel.value = (mapping.effect.apply)(channel, total_time, cue_time, delta);
//                 }
//             }
//         }
//     }
// }

fn apply_cue(fixtures: &mut [Fixture], cue: &Cue, total_time: f64) {
    // Apply cue-level static values
    for static_value in &cue.static_values {
        if let Some(fixture) = fixtures
            .iter_mut()
            .find(|f| f.name == static_value.fixture_name)
        {
            fixture.set_channel_value(&static_value.channel_name, static_value.value);
        }
    }

    // Apply chases
    for chase in &cue.chases {
        let chase_duration: f64 = chase.steps.iter().map(|step| step.duration).sum();
        let chase_time = total_time % chase_duration;
        let mut accumulated_time = 0.0;

        for step in &chase.steps {
            if chase_time >= accumulated_time && chase_time < accumulated_time + step.duration {
                let step_time = chase_time - accumulated_time;
                apply_chase_step(fixtures, step, total_time, step_time);
                break;
            }
            accumulated_time += step.duration;
        }
    }
}

fn apply_chase_step(fixtures: &mut [Fixture], step: &ChaseStep, total_time: f64, step_time: f64) {
    // Apply static values
    for static_value in &step.static_values {
        if let Some(fixture) = fixtures
            .iter_mut()
            .find(|f| f.name == static_value.fixture_name)
        {
            fixture.set_channel_value(&static_value.channel_name, static_value.value);
        }
    }

    // Apply effect mappings
    for mapping in &step.effect_mappings {
        let mut affected_fixtures: Vec<&mut Fixture> = fixtures
            .iter_mut()
            .filter(|f| mapping.fixture_names.contains(&f.name))
            .collect();

        for (i, fixture) in affected_fixtures.iter_mut().enumerate() {
            let should_apply = match mapping.distribution {
                EffectDistribution::All => true,
                EffectDistribution::Step(step) => i % step == 0,
                EffectDistribution::Wave(_) => true,
            };

            if should_apply {
                for channel_type in &mapping.channel_types {
                    if let Some(channel) = fixture.channels.iter_mut().find(|c| {
                        std::mem::discriminant(&c.channel_type)
                            == std::mem::discriminant(channel_type)
                    }) {
                        let phase_offset = match mapping.distribution {
                            EffectDistribution::Wave(phase) => i as f64 * phase,
                            _ => 0.0,
                        };
                        let progress = step_time / step.duration;
                        let effect_value = (mapping.effect.apply)(
                            channel.value,
                            total_time + phase_offset,
                            progress,
                            TARGET_DELTA,
                        );
                        // let constrained_value =
                        //     (effect_value * (mapping.effect.max - mapping.effect.min) as f64
                        //         + mapping.effect.min as f64) as u16;

                        let range = mapping.effect.max.saturating_sub(mapping.effect.min) as f64;
                        let constrained_value =
                            (effect_value * range + mapping.effect.min as f64) as u16;

                        channel.value = constrained_value;
                    }
                }
            }
        }
    }
}

fn linear_effect(_current: u16, _time: f64, progress: f64, _delta: f64) -> f64 {
    progress
}

fn sine_wave_effect(_current: u16, time: f64, _progress: f64, _delta: f64) -> f64 {
    (time.sin() * 0.5 + 0.5)
}

fn square_wave_effect(_current: u16, time: f64, _cue_time: f64, _delta: f64) -> f64 {
    if (time * TARGET_FREQUENCY).sin() > 0.0 {
        1.0
    } else {
        0.0
    }
}

fn sawtooth_wave_effect(_current: u16, time: f64, _cue_time: f64, _delta: f64) -> f64 {
    (time * TARGET_FREQUENCY) % 1.0
}

fn calculate_effect_value(beat_time: f64, cue_start_time: f64) -> u8 {
    let elapsed_time = beat_time - cue_start_time;
    let normalized_value = (elapsed_time.sin() + 1.0) / 2.0;
    (normalized_value * 255.0) as u8
}

fn beat_intensity(beat_time: f64) -> f64 {
    let beat_fraction = beat_time.fract();
    (1.0 - beat_fraction * 2.0).abs() // Creates a triangle wave that peaks on each beat
}

fn generate_dmx_data(fixtures: &[Fixture]) -> Vec<u8> {
    let mut dmx_data = vec![0; 512]; // Full DMX universe
    for fixture in fixtures {
        // let start = (fixture.start_address - 1) as usize;
        // let end = start + fixture.channels.len();
        // dmx_data[start..end].copy_from_slice(&fixture.get_dmx_values());

        let start_channel = (fixture.start_address - 1) as usize;
        let end_channel = (start_channel + fixture.channels.len()).min(dmx_data.len());
        let slice_length = end_channel - start_channel;
        dmx_data[start_channel..end_channel].copy_from_slice(&fixture.get_dmx_values());
    }
    dmx_data
}

fn send_dmx_data(
    //socket: &std::net::UdpSocket,
    socket: &UdpSocket,
    //target_addr: &str,
    broadcast_addr: SocketAddr,
    dmx_data: Vec<u8>,
) -> Result<(), Box<dyn Error>> {
    let command = ArtCommand::Output(Output {
        // length: dmx.len() as u16,
        //data: dmx.into(),
        //port_address: PortAddress::try_from(0).unwrap(),
        data: dmx_data.into(),
        ..Output::default()
    });

    let bytes = command.write_to_buffer().unwrap();
    socket.send_to(&bytes, broadcast_addr)?;
    Ok(())
}

fn key_pressed() -> io::Result<bool> {
    let mut buffer = [0; 1];
    Ok(io::stdin().read(&mut buffer)? > 0)
}

fn display_status(
    link: &AblLink,
    bpm: f64,
    frames_sent: u64,
    current_cue: &str,
    elapsed: f64,
    cue_time: f64,
) {
    //let bpm = state.tempo();
    let num_peers = link.num_peers();

    print!("\r"); // Move cursor to the beginning of the line
    print!(
        "Frames: {:8} | BPM: {:6.2} | Peers: {:3} | Current Cue: {:3} | Elapsed: {:6.2}s | Cue Time: {:6.2}s | FPS: {:5.2}",
        frames_sent, bpm, num_peers, current_cue, elapsed, cue_time, frames_sent as f64 / elapsed
    );
    stdout().flush().unwrap();
}
