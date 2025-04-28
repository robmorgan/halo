use anyhow::Result;
use serde_json::{from_reader, to_writer_pretty};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::LightingConsole;

use super::show::Show;

pub struct ShowManager {
    shows_directory: PathBuf,
    current_show: Option<Show>,
    current_path: Option<PathBuf>,
}

impl ShowManager {
    pub fn new() -> Result<Self> {
        // Get the current working directory
        let shows_dir = std::env::current_dir()?;

        Ok(Self {
            shows_directory: shows_dir,
            current_show: None,
            current_path: None,
        })
    }

    pub fn new_show(&mut self, name: String) -> Show {
        let show = Show::new(name);
        self.current_show = Some(show.clone());
        self.current_path = None;
        show
    }

    pub fn save_show(&mut self, console: &LightingConsole) -> Result<PathBuf> {
        let show = if let Some(show) = &mut self.current_show {
            // Update with latest console state
            show.fixtures = console.fixtures.clone();
            show.cue_lists = console.cue_manager.get_cue_lists().clone();
            show.modified_at = SystemTime::now();
            show.clone()
        } else {
            // Create a new show if none exists
            Show::from_console(console, "Untitled Show".to_string())
        };

        let path = if let Some(path) = &self.current_path {
            path.clone()
        } else {
            // Create a new file path based on show name
            let sanitized_name = show.name.replace(" ", "_").to_lowercase();
            self.shows_directory
                .join(format!("{}.halo", sanitized_name))
        };

        // Save to disk
        let file = File::create(&path)?;
        to_writer_pretty(file, &show)?;

        self.current_show = Some(show);
        self.current_path = Some(path.clone());

        Ok(path)
    }

    pub fn save_show_as(
        &mut self,
        console: &LightingConsole,
        name: String,
        path: PathBuf,
    ) -> Result<PathBuf> {
        let mut show = Show::from_console(console, name);
        show.modified_at = SystemTime::now();

        let file = File::create(&path)?;
        to_writer_pretty(file, &show)?;

        self.current_show = Some(show);
        self.current_path = Some(path.clone());

        Ok(path)
    }

    pub fn load_show(&mut self, path: &Path) -> Result<Show> {
        let file = File::open(path)?;
        let show: Show = from_reader(file)?;

        self.current_show = Some(show.clone());
        self.current_path = Some(path.to_path_buf());

        Ok(show)
    }

    pub fn apply_show_to_console(&self, console: &mut LightingConsole) -> Result<()> {
        if let Some(show) = &self.current_show {
            // Apply show data to console
            console.fixtures = show.fixtures.clone();
            console.cue_manager.set_cue_lists(show.cue_lists.clone());
            Ok(())
        } else {
            Err(anyhow::anyhow!("No show is currently loaded"))
        }
    }

    pub fn list_shows(&self) -> Result<Vec<PathBuf>> {
        let entries = fs::read_dir(&self.shows_directory)?;

        let mut shows = Vec::new();
        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().map_or(false, |ext| ext == "halo") {
                shows.push(path);
            }
        }

        Ok(shows)
    }
}
