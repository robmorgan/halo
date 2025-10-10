use halo_fixtures::ChannelType;

use crate::{EffectMapping, StaticValue};

#[derive(Clone)]
pub struct Programmer {
    values: Vec<StaticValue>,
    effects: Vec<EffectMapping>,
    preview_mode: bool,
    collapsed: bool,
    selected_fixtures: Vec<usize>,
}

impl Programmer {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            effects: Vec::new(),
            preview_mode: false,
            collapsed: false,
            selected_fixtures: Vec::new(),
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

    pub fn set_preview_mode(&mut self, preview_mode: bool) {
        self.preview_mode = preview_mode;
    }

    pub fn get_preview_mode(&self) -> bool {
        self.preview_mode
    }

    pub fn clear(&mut self) {
        self.values.clear();
        self.effects.clear();
    }

    pub fn set_collapsed(&mut self, collapsed: bool) {
        self.collapsed = collapsed;
    }

    pub fn get_collapsed(&self) -> bool {
        self.collapsed
    }

    pub fn set_selected_fixtures(&mut self, fixtures: Vec<usize>) {
        self.selected_fixtures = fixtures;
    }

    pub fn add_selected_fixture(&mut self, fixture_id: usize) {
        if !self.selected_fixtures.contains(&fixture_id) {
            self.selected_fixtures.push(fixture_id);
        }
    }

    pub fn remove_selected_fixture(&mut self, fixture_id: usize) {
        self.selected_fixtures.retain(|&id| id != fixture_id);
    }

    pub fn clear_selected_fixtures(&mut self) {
        self.selected_fixtures.clear();
    }

    pub fn get_selected_fixtures(&self) -> &Vec<usize> {
        &self.selected_fixtures
    }
}
