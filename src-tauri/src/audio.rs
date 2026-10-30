use std::process::{Command, Stdio};

const SOURCE: &str = "@DEFAULT_AUDIO_SOURCE@";
const SOUNDS: &str = "/usr/share/sounds/freedesktop/stereo";

pub fn parse_muted(out: &str) -> Option<bool> {
    let line = out.lines().find(|l| l.starts_with("Volume:"))?;
    Some(line.contains("[MUTED]"))
}

pub fn get_muted() -> Option<bool> {
    let out = Command::new("wpctl").args(["get-volume", SOURCE]).output().ok()?;
    parse_muted(&String::from_utf8_lossy(&out.stdout))
}

pub fn set_muted(muted: bool) -> bool {
    Command::new("wpctl")
        .args(["set-mute", SOURCE, if muted { "1" } else { "0" }])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn command_text(muted: bool) -> String {
    format!("wpctl set-mute {SOURCE} {}", muted as u8)
}

// ponytail: system theme sounds; ship our own .oga if these feel off
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
}
