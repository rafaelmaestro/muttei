use serde::Serialize;
use serde_json::Value;
#[cfg(target_os = "linux")]
use std::process::{Command, Stdio};
#[cfg(target_os = "macos")]
pub use crate::mac::{get_muted, mics, play, set_default, set_muted};
#[cfg(windows)]
pub use crate::win::{get_muted, mics, play, set_default, set_muted};

pub const DEFAULT: &str = "@DEFAULT_AUDIO_SOURCE@";
#[cfg(target_os = "linux")]
const SOUNDS: &str = "/usr/share/sounds/freedesktop/stereo";

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_muted(out: &str) -> Option<bool> {
    let line = out.lines().find(|l| l.starts_with("Volume:"))?;
    Some(line.contains("[MUTED]"))
}

#[cfg(target_os = "linux")]
pub fn get_muted(target: &str) -> Option<bool> {
    let out = Command::new("wpctl").args(["get-volume", target]).output().ok()?;
    parse_muted(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(target_os = "linux")]
pub fn set_muted(target: &str, muted: bool) -> bool {
    Command::new("wpctl")
        .args(["set-mute", target, if muted { "1" } else { "0" }])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn command_text(target: &str, muted: bool) -> String {
    if cfg!(windows) {
        format!("IAudioEndpointVolume::SetMute({}) · {target}", muted as u8)
    } else if cfg!(target_os = "macos") {
        format!("kAudioDevicePropertyMute = {} · {target}", muted as u8)
    } else {
        format!("wpctl set-mute {target} {}", muted as u8)
    }
}

pub fn read_text(target: &str) -> String {
    if cfg!(windows) {
        format!("IAudioEndpointVolume::GetMute · {target}")
    } else if cfg!(target_os = "macos") {
        format!("kAudioDevicePropertyMute? · {target}")
    } else {
        format!("wpctl get-volume {target}")
    }
}

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Mic {
    pub id: String,
    pub name: String,
    pub desc: String,
    pub def: bool,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_mics(dump: &str) -> Vec<Mic> {
    let Ok(Value::Array(objs)) = serde_json::from_str(dump) else { return vec![] };
    let def = objs
        .iter()
        .filter(|o| o["props"]["metadata.name"] == "default")
        .flat_map(|o| o["metadata"].as_array().cloned().unwrap_or_default())
        .find(|m| m["key"] == "default.audio.source")
        .and_then(|m| m["value"]["name"].as_str().map(String::from));
    objs.iter()
        .filter(|o| o["info"]["props"]["media.class"] == "Audio/Source")
        .filter_map(|o| {
            let p = &o["info"]["props"];
            let name = p["node.name"].as_str()?.to_string();
            Some(Mic {
                id: o["id"].as_u64()?.to_string(),
                desc: p["node.description"].as_str().unwrap_or(&name).split_whitespace().collect::<Vec<_>>().join(" "),
                def: def.as_deref() == Some(name.as_str()),
                name,
            })
        })
        .collect()
}

// wpctl only takes numeric ids, which change on replug, so the UI and the phone pick by node.name
#[cfg(target_os = "linux")]
pub fn mics() -> Vec<Mic> {
    Command::new("pw-dump").output().map(|o| parse_mics(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default()
}

pub fn pick<'a>(mics: &'a [Mic], name: &str) -> Option<&'a Mic> {
    mics.iter().find(|m| m.name == name)
}

#[cfg(target_os = "linux")]
pub fn set_default(id: &str) -> bool {
    Command::new("wpctl").args(["set-default", id]).status().is_ok_and(|s| s.success())
}

pub fn default_text(id: &str) -> String {
    if cfg!(target_os = "linux") {
        format!("wpctl set-default {id}")
    } else {
        format!("padrão de entrada → {id}")
    }
}

// ponytail: system theme sounds; ship our own .oga if these feel off
#[cfg(target_os = "linux")]
pub fn play(muted: bool) {
    let file = if muted { "device-removed.oga" } else { "device-added.oga" };
    let _ = Command::new("pw-play")
        .arg(format!("{SOUNDS}/{file}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wpctl_output() {
        assert_eq!(parse_muted("Volume: 0.94\n"), Some(false));
        assert_eq!(parse_muted("Volume: 0.94 [MUTED]\n"), Some(true));
        assert_eq!(parse_muted("Translate ID error: '@DEFAULT_AUDIO_SOURCE@' is not a valid ID\n"), None);
        assert_eq!(parse_muted(""), None);
    }

    #[test]
    fn lists_mics_and_picks_by_name() {
        let dump = r#"[
          {"id":42,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Source","node.name":"usb-mic","node.description":"fifine  Microphone"}}},
          {"id":57,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Source","node.name":"headset","node.description":"Headset"}}},
          {"id":59,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Sink","node.name":"hdmi"}}},
          {"id":36,"type":"PipeWire:Interface:Metadata","props":{"metadata.name":"default"},"metadata":[{"key":"default.audio.source","value":{"name":"usb-mic"}}]}
        ]"#;
        let m = parse_mics(dump);
        assert_eq!(m.len(), 2);
        assert_eq!((m[0].desc.as_str(), m[0].def, m[1].def), ("fifine Microphone", true, false));
        assert!(parse_mics("not json").is_empty());
        assert_eq!(pick(&m, "headset").map(|x| x.id.as_str()), Some("57"));
        assert_eq!(pick(&m, "../../etc"), None);
    }
}
