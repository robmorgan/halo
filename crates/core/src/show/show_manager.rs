use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{from_reader, to_writer_pretty};

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

    pub fn save_show(&mut self, show: &Show) -> Result<PathBuf> {
        let path = if let Some(path) = &self.current_path {
            path.clone()
        } else {
            // Create a new file path based on show name
            let sanitized_name = show.name.replace(" ", "_").to_lowercase();
            self.shows_directory
                .join(format!("{}.json", sanitized_name))
        };

        // Save to disk
        let file = File::create(&path)?;
        to_writer_pretty(file, &show)?;

        self.current_show = Some(show.clone());
        self.current_path = Some(path.clone());

        Ok(path)
    }

    pub fn save_show_as(&mut self, show: &Show, path: PathBuf) -> Result<PathBuf> {
        let file = File::create(&path)?;
        to_writer_pretty(file, &show)?;

        self.current_show = Some(show.clone());
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

    pub fn list_shows(&self) -> Result<Vec<PathBuf>> {
        let entries = fs::read_dir(&self.shows_directory)?;

        let mut shows = Vec::new();
        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                shows.push(path);
            }
        }

        Ok(shows)
    }
}
