#[derive(Clone, Debug)]
pub struct CueList {
    pub priority: u8,
    pub name: String,
    pub cues: Vec<Cue>,
    // processed_cues
    // active_cue
}

#[derive(Clone, Debug)]
pub struct CueMaster {
    pub cue_lists: Vec<CueList>,
    //pub clock: Clock,
    // fixture_library: FixtureLibrary, (Fixture Manager)
}

impl CueMaster {
    pub fn new(cue_lists: Vec<CueList>) -> Self {
        CueMaster {
            cue_lists,
            //clock: Clock::new(),
            //fixture_library: FixtureLibrary::new(),
        }
    }
}
