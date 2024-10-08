use serde::{Deserialize, Serialize};

use crate::language::Language;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub token: String,
    pub prefix: String,
    pub languages: Vec<Language>,
    pub owner: u64,
}

impl Config {
    pub fn get_language(&self, name: &String) -> Option<Language> {
        for language in self.languages.iter() {
            if language.code.contains(name) {
                return Some(language.clone());
            }
        }

        None
    }
}
