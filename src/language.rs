use serde::Deserialize;

use std::collections::HashMap;
use std::fs::{read_dir, read_to_string};
use std::io;
use std::path::Path;

const LANGUAGES_DIR: &'static str = "langs";

#[derive(Deserialize, Debug, Clone)]
pub struct Language {
    pub name: String, // Display name
    pub version: String,
    pub exec: String,
    pub args: String,
    pub add_mem_limit: f64,
    pub add_time_limit: f64,
}

#[derive(Debug, Clone)]
pub struct Languages {
    langs: HashMap<String, Language>,
}

impl Languages {
    pub fn load() -> io::Result<Self> {
        let binding = format!("./{}", LANGUAGES_DIR).clone();
        let dir = Path::new(&binding);
        assert_eq!(dir.is_dir(), true);
        let mut map = HashMap::new();
        for entry in read_dir(dir)? {
            let entry = entry?;
            if let Ok(file_t) = entry.file_type() {
                if file_t.is_file() {
                    let path = entry.path();
                    let s = read_to_string(path).expect("Some error occured");
                    if let Ok(lang) = toml::from_str::<Language>(&s) {
                        map.insert(lang.name.clone(), lang.clone());
                    }
                }
            }
        }
        Ok(Self { langs: map })
    }

    pub fn get(&self, name: String) -> Option<&Language> {
        self.langs.get(&name)
    }
}
