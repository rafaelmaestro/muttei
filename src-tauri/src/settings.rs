use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const TOGGLE_MUTE: &str = "toggle_mute";
pub const DEFAULT_SHORTCUT: &str = "Ctrl+Shift+M";
pub const ACTIONS: &[(&str, &str)] = &[
    (TOGGLE_MUTE, "Mutar e desmutar"),
    ("ptt", "Segurar para falar"),
    ("ptc", "Segurar para tossir"),
    ("camera", "Câmera on/off"),
    ("hand", "Levantar a mão"),
    ("leave", "Sair da chamada"),
];
const TAKEN: &[(&str, &str)] = &[
    ("Super+L", "Super+L é do GNOME (travar a tela)."),
    ("Ctrl+Alt+T", "Ctrl+Alt+T já abre o Terminal no GNOME."),
];

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub when: String,
    pub token: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Browser {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Settings {
    pub shortcuts: BTreeMap<String, String>,
    pub sound: bool,
    pub pill: bool,
    pub dot: bool,
    pub dot_corner: String,
    pub autostart: bool,
    pub lock_rule: bool,
    pub app_rule: bool,
    pub apps: Vec<String>,
    pub phone_on: bool,
    pub iface: Option<String>,
    pub port: u16,
    pub devices: Vec<Device>,
    pub meet_browsers: Vec<Browser>,
    pub intro_seen: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shortcuts: BTreeMap::from([(TOGGLE_MUTE.into(), DEFAULT_SHORTCUT.into())]),
            sound: true,
            pill: true,
            dot: true,
            dot_corner: "tr".into(),
            autostart: true,
            lock_rule: true,
            app_rule: true,
            apps: vec!["zoom".into()],
            phone_on: false,
            iface: None,
            port: 47821,
            devices: vec![],
            meet_browsers: vec![],
            intro_seen: false,
        }
    }
}

impl Settings {
    fn path() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
        base.join("muttei").join("settings.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let p = Self::path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, serde_json::to_vec_pretty(self).unwrap()).is_ok() {
            let _ = std::fs::rename(tmp, p);
        }
    }

    pub fn shortcut(&self) -> String {
        self.shortcuts.get(TOGGLE_MUTE).cloned().unwrap_or_else(|| DEFAULT_SHORTCUT.into())
    }

    pub fn validate_shortcut(&self, action: &str, combo: &str) -> Result<(), String> {
        if !ACTIONS.iter().any(|(a, _)| *a == action) {
            return Err(format!("Ação desconhecida: {action}"));
        }
        if let Some((_, why)) = TAKEN.iter().find(|(k, _)| *k == combo) {
            return Err(why.to_string());
        }
        if !combo.contains('+') {
            return Err("Use pelo menos um modificador (Ctrl, Alt, Shift ou Super).".into());
        }
        if let Some((other, _)) = self.shortcuts.iter().find(|(a, c)| a.as_str() != action && c.as_str() == combo) {
            let label = ACTIONS.iter().find(|(a, _)| a == other).map(|(_, l)| *l).unwrap_or(other);
            return Err(format!("{combo} já é o atalho de \"{label}\"."));
        }
        Ok(())
    }

    pub fn merge(&self, patch: serde_json::Value) -> Result<Self, String> {
        let mut v = serde_json::to_value(self).map_err(|e| e.to_string())?;
        if let (Some(obj), Some(p)) = (v.as_object_mut(), patch.as_object()) {
            for (k, val) in p {
                obj.insert(k.clone(), val.clone());
            }
        }
        serde_json::from_value(v).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_keeps_untouched_fields() {
        let s = Settings::default();
        let n = s.merge(serde_json::json!({"sound": false, "apps": ["discord"]})).unwrap();
        assert!(!n.sound);
        assert_eq!(n.apps, vec!["discord"]);
        assert_eq!(n.shortcut(), DEFAULT_SHORTCUT);
    }

    #[test]
    fn validates_shortcuts() {
        let mut s = Settings::default();
        assert!(s.validate_shortcut("ptt", "Ctrl+Alt+Space").is_ok());
        s.shortcuts.insert("ptt".into(), "Ctrl+Alt+Space".into());
        assert!(s.validate_shortcut("camera", "Ctrl+Alt+Space").unwrap_err().contains("Segurar para falar"));
        assert!(s.validate_shortcut("ptt", "Ctrl+Alt+Space").is_ok());
        assert!(s.validate_shortcut("hand", "Ctrl+Shift+M").unwrap_err().contains("Mutar"));
        assert!(s.validate_shortcut("hand", "Super+L").is_err());
        assert!(s.validate_shortcut("hand", "H").is_err());
        assert!(s.validate_shortcut("nope", "Ctrl+H").is_err());
        assert!(s.validate_shortcut("ptt", "Ctrl+Space").is_ok());
    }

    #[test]
    fn old_file_without_new_fields_loads() {
        let s: Settings = serde_json::from_str(r#"{"sound":false}"#).unwrap();
        assert!(!s.sound && s.pill && s.port == 47821);
    }
}
