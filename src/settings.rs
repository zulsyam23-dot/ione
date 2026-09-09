//! Tiny persisted app state: the editor theme/font names (settings.txt) and
//! the recent-files list (recent.txt), both under `%APPDATA%\ione`. Plain
//! text, one key per line; a missing/corrupt file is simply ignored. Set
//! `IONE_CONFIG_DIR` to redirect the directory (used by the tests).

use std::collections::HashSet;
use std::path::PathBuf;

const DIR_OVERRIDE: &str = "IONE_CONFIG_DIR";
const SETTINGS_FILE: &str = "settings.txt";
const RECENT_FILE: &str = "recent.txt";
const MAX_RECENT: usize = 10;

pub struct Settings {
    pub theme: Option<String>,
    pub font: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: None,
            font: None,
        }
    }
}

fn dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var(DIR_OVERRIDE) {
        return Some(PathBuf::from(d));
    }
    std::env::var("APPDATA")
        .map(|a| PathBuf::from(a).join("ione"))
        .ok()
}

pub fn load() -> Settings {
    let mut s = Settings::default();
    let Some(d) = dir() else { return s };
    let Ok(txt) = std::fs::read_to_string(d.join(SETTINGS_FILE)) else {
        return s;
    };
    for line in txt.lines() {
        if let Some(v) = line.strip_prefix("theme=") {
            s.theme = Some(v.to_string());
        } else if let Some(v) = line.strip_prefix("font=") {
            s.font = Some(v.to_string());
        }
    }
    s
}

impl Settings {
    pub fn save(&self) {
        let Some(d) = dir() else { return };
        let _ = std::fs::create_dir_all(&d);
        let mut txt = String::new();
        if let Some(t) = &self.theme {
            txt.push_str(&format!("theme={t}\n"));
        }
        if let Some(f) = &self.font {
            txt.push_str(&format!("font={f}\n"));
        }
        let _ = std::fs::write(d.join(SETTINGS_FILE), txt);
    }
}

pub fn load_recents() -> Vec<PathBuf> {
    let Some(d) = dir() else { return Vec::new() };
    let Ok(txt) = std::fs::read_to_string(d.join(RECENT_FILE)) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    txt.lines()
        .map(PathBuf::from)
        .filter(|p| p.is_file() && seen.insert(p.clone()))
        .take(MAX_RECENT)
        .collect()
}

pub fn save_recents(list: &[PathBuf]) {
    let Some(d) = dir() else { return };
    let _ = std::fs::create_dir_all(&d);
    let mut txt = String::new();
    for p in list.iter().take(MAX_RECENT) {
        txt.push_str(&p.to_string_lossy());
        txt.push('\n');
    }
    let _ = std::fs::write(d.join(RECENT_FILE), txt);
}

/// Push `path` to the front of the list (deduped, capped), so the most
/// recently opened file is always first. Bumps the persisted copy, too.
pub fn push_recent(list: &mut Vec<PathBuf>, path: PathBuf) {
    list.retain(|p| p != &path);
    list.insert(0, path);
    list.truncate(MAX_RECENT);
    save_recents(list);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_roundtrip_via_config_dir() {
        let dir = std::env::temp_dir().join(format!("ione-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Safety: single-threaded test, no other code reads this variable.
        unsafe { std::env::set_var(DIR_OVERRIDE, &dir) };
        let mut list = Vec::new();
        for i in 0..12 {
            let p = dir.join(format!("f{i}.rs"));
            std::fs::write(&p, "").unwrap();
            push_recent(&mut list, p);
        }
        save_recents(&list);
        let loaded = load_recents();
        assert_eq!(loaded, list);
        assert_eq!(loaded.len(), MAX_RECENT);
        assert_eq!(
            loaded[0].file_name().unwrap().to_string_lossy(),
            "f11.rs"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}