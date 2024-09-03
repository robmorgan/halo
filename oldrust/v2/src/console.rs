mod effect;
mod fixture;

pub struct Console {
    fixture_manager: FixtureManager,
    effect_library: EffectLibrary,
    link: Link,
}

impl Console {
    pub fn new() -> Self {
        Console {
            fixture_manager: FixtureManager::new(),
            effect_library: EffectLibrary::new(),
        }
    }

    pub fn run(&mut self) {
        // Main loop for the console
        loop {
            // Handle user input
            // Update lighting state
            // Apply effects
            // Render output
        }
    }

    // Additional methods for console functionality
}
