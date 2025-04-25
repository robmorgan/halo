use halo_fixtures::ChannelType;

use crate::{EffectMapping, StaticValue};

pub struct Programmer {
    values: Vec<StaticValue>,
    effects: Vec<EffectMapping>,
}

#[derive(Clone, Debug)]
pub struct ProgrammerValue {
    pub fixture_id: usize,
    pub channel_type: ChannelType,
    pub value: u8,
}

impl Programmer {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            effects: Vec::new(),
        }
    }

    pub fn add_value(&mut self, fixture_id: usize, channel_type: ChannelType, value: u8) {
        // Remove any existing value for this fixture/channel combination
        self.values
            .retain(|v| !(v.fixture_id == fixture_id && v.channel_type == channel_type));

        // Add the new value
        self.values.push(StaticValue {
            fixture_id,
            channel_type,
            value,
        });
    }

    pub fn get_values(&self) -> &Vec<StaticValue> {
        &self.values
    }

    pub fn add_effect(&mut self, effect: EffectMapping) {
        self.effects.push(effect);
    }

    pub fn get_effects(&self) -> &Vec<EffectMapping> {
        &self.effects
    }

    pub fn clear(&mut self) {
        self.values.clear();
        self.effects.clear();
    }
}
