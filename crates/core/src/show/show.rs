use serde::{Deserialize, Serialize};
use std::time::SystemTime;

use halo_fixtures::Fixture;

use crate::{CueList, LightingConsole};

#[derive(Serialize, Deserialize, Clone)]
pub struct Show {
    pub name: String,
    pub created_at: SystemTime,
    pub modified_at: SystemTime,
    pub fixtures: Vec<Fixture>,
    pub cue_lists: Vec<CueList>,
    pub version: String, // Schema version for future compatibility
}

impl Show {
    pub fn new(name: String) -> Self {
        let now = SystemTime::now();
        Self {
            name,
            created_at: now,
            modified_at: now,
            fixtures: Vec::new(),
            cue_lists: Vec::new(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    pub fn from_console(console: &LightingConsole, name: String) -> Self {
        let mut show = Self::new(name);
        show.fixtures = console.fixtures.clone();
        show.cue_lists = console.cue_manager.get_cue_lists().clone();
        show
    }
}
