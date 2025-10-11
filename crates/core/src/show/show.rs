use std::time::SystemTime;

use halo_fixtures::Fixture;
use serde::{Deserialize, Serialize};

use crate::{CueList, Settings};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Show {
    pub name: String,
    pub created_at: SystemTime,
    pub modified_at: SystemTime,
    pub fixtures: Vec<Fixture>,
    pub cue_lists: Vec<CueList>,
    pub settings: Settings,
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
            settings: Settings::default(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}
