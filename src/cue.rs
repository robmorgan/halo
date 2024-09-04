use crate::effect::Effect;
use crate::fixture::ChannelType;

#[derive(Clone, Debug)]
pub struct Cue {
    pub name: String,
    pub duration: f64,
    pub static_values: Vec<StaticValue>,
    pub chases: Vec<Chase>,
}

#[derive(Clone, Debug)]
pub struct StaticValue {
    pub fixture_name: String,
    pub channel_name: String,
    pub value: u16,
}

#[derive(Clone, Debug)]
pub struct Chase {
    pub name: String,
    pub steps: Vec<ChaseStep>,
    pub loop_count: Option<usize>, // None for infinite loop
}

#[derive(Clone, Debug)]
pub struct ChaseStep {
    pub duration: f64,
    pub effect_mappings: Vec<EffectMapping>,
    pub static_values: Vec<StaticValue>,
}

// TODO - one day we'll make this apply to multiple fixtures and channels
// TODO - this might be the case now
#[derive(Clone, Debug)]
pub struct EffectMapping {
    pub effect: Effect,
    pub fixture_names: Vec<String>,
    pub channel_types: Vec<ChannelType>,
    pub distribution: EffectDistribution,
}

#[derive(Clone, Debug)]
pub enum EffectDistribution {
    All,
    Step(usize),
    Wave(f64), // Phase offset between fixtures
}
