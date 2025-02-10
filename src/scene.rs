use crate::cue::Cue;
use crate::effect::Effect;
use crate::fixture::PatchedFixture;
use std::collections::HashMap;

// TODO - add a method to get all of the DMX values for a frame.
// The scene should loop through all static values and then apply all effects.
// Lastly the console will process any active overrides to calculate the final DMX values.

pub struct Scene {
    pub fixtures: Vec<PatchedFixture>,
    pub static_values: HashMap<String, f64>,
    pub effects: Vec<Effect>,
}

impl Scene {
    pub fn new() -> Self {
        Scene {
            fixtures: Vec::new(),
            static_values: HashMap::new(),
            effects: Vec::new(),
        }
    }

    pub fn from_cue(cue: &Cue) -> Self {
        let mut scene = Scene::new();
        scene.static_values = cue.static_values.clone();
        scene.effects = cue.effects.clone();
        scene
    }

    pub fn add_fixture(&mut self, fixture: PatchedFixture) {
        self.fixtures.push(fixture);
    }

    pub fn add_effect(&mut self, effect: Effect) {
        self.effects.push(effect);
    }

    pub fn set_static_value(&mut self, parameter: String, value: f64) {
        self.static_values.insert(parameter, value);
    }

    pub fn get_static_value(&self, parameter: &str) -> Option<&f64> {
        self.static_values.get(parameter)
    }
}
