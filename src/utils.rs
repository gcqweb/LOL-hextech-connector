use crate::models::{Hero, Settings};
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};

// --- Constants ---
pub const APP_TITLE: &str = "海克斯 • 连接器";
pub const DATA_DIR: &str = "data";
pub const IMG_DIR: &str = "data/img";
pub const SETTINGS_FILE: &str = "settings.json";
pub const CHAMP_FILE: &str = "data/champion.json";
pub const VERSION_FILE: &str = "data/version.txt";

// --- Helper Functions ---
pub fn load_settings() -> Settings {
    if let Ok(content) = fs::read_to_string(SETTINGS_FILE) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Settings::default()
    }
}

pub fn save_settings_to_disk(s: &Settings) {
    let _ = fs::create_dir_all(DATA_DIR);
    if let Ok(json) = serde_json::to_string_pretty(s) {
        let _ = fs::write(SETTINGS_FILE, json);
    }
}

pub fn lookup_hero_name_by_id(heroes: &Arc<Mutex<HashMap<i32, Hero>>>, id: i32) -> String {
    if id == 0 {
        return String::new();
    }
    let map = heroes.lock().unwrap();
    map.get(&id).map(|h| h.title.clone()).unwrap_or_default()
}
