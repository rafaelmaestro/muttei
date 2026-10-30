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

#[derive(Serialize, Clone, PartialEq, Debug, Default)]
pub struct Mic {
    pub id: String,
    pub name: String,
    pub desc: String,
    pub def: bool,
    /// a headset whose current profile has no mic; picking it switches the profile first
    pub headset: bool,
    #[serde(skip)]
    pub dev: String,
    /// (profile with a mic, profile it is in now)
    #[serde(skip)]
    pub profile: Option<(String, String)>,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_mics(dump: &str) -> Vec<Mic> {
    // while the graph changes pw-dump appends update documents after the first array; only the first is the full dump
    let Some(Ok(Value::Array(objs))) = serde_json::Deserializer::from_str(dump).into_iter::<Value>().next() else { return vec![] };
    let def = objs
        .iter()
        .filter(|o| o["props"]["metadata.name"] == "default")
        .flat_map(|o| o["metadata"].as_array().cloned().unwrap_or_default())
        .find(|m| m["key"] == "default.audio.source")
        .and_then(|m| m["value"]["name"].as_str().map(String::from));
    let tidy = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut mics: Vec<Mic> = objs
        .iter()
        .filter(|o| o["info"]["props"]["media.class"] == "Audio/Source")
        .filter_map(|o| {
            let p = &o["info"]["props"];
            let name = p["node.name"].as_str()?.to_string();
            Some(Mic {
                id: o["id"].as_u64()?.to_string(),
                desc: tidy(p["node.description"].as_str().unwrap_or(&name)),
                def: def.as_deref() == Some(name.as_str()),
                dev: p["device.id"].as_u64().map(|d| d.to_string()).unwrap_or_default(),
                name,
                ..Default::default()
            })
        })
        .collect();
    // Bluetooth headsets in music mode (A2DP) expose no mic until they switch to headset mode (HFP)
    let has_mic = |p: &Value| p["classes"].as_array().is_some_and(|c| c.iter().any(|c| c[0] == "Audio/Source"));
    for o in objs.iter().filter(|o| o["info"]["props"]["media.class"] == "Audio/Device") {
        let (Some(id), p, params) = (o["id"].as_u64().map(|d| d.to_string()), &o["info"]["props"], &o["info"]["params"]) else { continue };
        let Some(now) = params["Profile"].get(0) else { continue };
        if has_mic(now) || mics.iter().any(|m| m.dev == id) {
            continue;
        }
        let profiles = params["EnumProfile"].as_array().cloned().unwrap_or_default();
        let usable: Vec<&Value> = profiles.iter().filter(|e| e["available"] != "no" && has_mic(e)).collect();
        let Some(to) = usable.iter().find(|e| e["name"] == "headset-head-unit").or(usable.first()) else { continue };
        let (Some(name), Some(to), Some(now)) = (p["device.name"].as_str(), to["index"].as_u64(), now["index"].as_u64()) else { continue };
        mics.push(Mic {
            id: format!("profile:{id}"),
            name: name.into(),
            desc: tidy(p["device.description"].as_str().unwrap_or(name)),
            headset: true,
            dev: id,
            profile: Some((to.to_string(), now.to_string())),
            ..Default::default()
        });
    }
    mics
}

#[cfg(target_os = "linux")]
pub fn set_profile(dev: &str, index: &str) -> bool {
    Command::new("wpctl").args(["set-profile", dev, index]).status().is_ok_and(|s| s.success())
}

#[cfg(not(target_os = "linux"))]
pub fn set_profile(_: &str, _: &str) -> bool {
    false
}

/// For a headset entry, switches it to its mic profile and returns the mic that shows up.
pub fn activate(m: &Mic) -> Option<Mic> {
    let Some((to, _)) = &m.profile else { return Some(m.clone()) };
    if !set_profile(&m.dev, to) {
        return None;
    }
    // the HFP source node appears a moment after the profile switch
    (0..25).find_map(|_| {
        std::thread::sleep(std::time::Duration::from_millis(200));
        mics().into_iter().find(|x| x.dev == m.dev && x.profile.is_none())
    })
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
        assert_eq!(parse_mics(&format!("{dump}\n[{{\"id\":42,\"info\":null}}]")).len(), 2);
        assert_eq!(pick(&m, "headset").map(|x| x.id.as_str()), Some("57"));
    }

    #[test]
    fn lists_headsets_whose_mic_needs_a_profile_switch() {
        let card = |id: u32, name: &str, profile: &str| {
            format!(
                r#"{{"id":{id},"type":"PipeWire:Interface:Device","info":{{"props":{{"media.class":"Audio/Device","device.name":"{name}","device.description":"Buds"}},
                "params":{{"Profile":[{profile}],"EnumProfile":[
                  {{"index":3,"name":"headset-head-unit","available":"yes","classes":[["Audio/Source",1],["Audio/Sink",1]]}},
                  {{"index":5,"name":"a2dp-sink","available":"yes","classes":[["Audio/Sink",1]]}}]}}}}}}"#
            )
        };
        let a2dp = r#"{"index":5,"name":"a2dp-sink","classes":[["Audio/Sink",1]]}"#;
        let hfp = r#"{"index":3,"name":"headset-head-unit","classes":[["Audio/Source",1],["Audio/Sink",1]]}"#;
        let m = parse_mics(&format!("[{}]", card(114, "bluez_card.buds", a2dp)));
        assert_eq!(m.len(), 1);
        assert!(m[0].headset);
        assert_eq!(m[0].profile, Some(("3".into(), "5".into())));
        let src = r#"{"id":120,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Source","node.name":"bluez_input.buds","node.description":"Buds","device.id":114}}}"#;
        let m = parse_mics(&format!("[{},{src}]", card(114, "bluez_card.buds", hfp)));
        assert_eq!(m.iter().map(|x| (x.name.as_str(), x.headset)).collect::<Vec<_>>(), vec![("bluez_input.buds", false)]);
        assert_eq!(pick(&m, "../../etc"), None);
    }
}
