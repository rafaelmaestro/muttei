use std::collections::HashSet;
#[cfg(target_os = "linux")]
use std::io::{BufRead, BufReader};
use std::path::Path;
#[cfg(target_os = "linux")]
use std::process::{Command, Stdio};
#[cfg(target_os = "linux")]
use std::time::Duration;
#[cfg(windows)]
pub use crate::win::{lock_now, running, watch_lock};

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn lock_signal(line: &str) -> Option<bool> {
    if !line.contains("ActiveChanged") {
        return None;
    }
    Some(line.contains("(true,)"))
}

#[cfg(target_os = "linux")]
pub fn watch_lock(on_change: impl Fn(bool) + Send + 'static) {
    std::thread::spawn(move || loop {
        let child = Command::new("gdbus")
            .args(["monitor", "--session", "--dest", "org.gnome.ScreenSaver", "--object-path", "/org/gnome/ScreenSaver"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(mut c) = child {
            if let Some(out) = c.stdout.take() {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    if let Some(locked) = lock_signal(&line) {
                        on_change(locked);
                    }
                }
            }
            let _ = c.wait();
        }
        std::thread::sleep(Duration::from_secs(5));
    });
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn base(s: &str) -> Option<String> {
    let b = Path::new(s.split('\0').next()?).file_name()?.to_str()?;
    (!b.is_empty()).then(|| b.to_lowercase())
}

#[cfg(target_os = "linux")]
pub fn lock_now() {
    let _ = Command::new("loginctl").arg("lock-session").spawn();
}

#[cfg(target_os = "linux")]
pub fn running() -> HashSet<String> {
    let mut names = HashSet::new();
    let Ok(dir) = std::fs::read_dir("/proc") else { return names };
    for e in dir.flatten() {
        let p = e.path();
        if !e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if let Some(b) = std::fs::read_link(p.join("exe")).ok().and_then(|x| base(&x.to_string_lossy())) {
            names.insert(b);
        }
        if let Some(b) = std::fs::read(p.join("cmdline")).ok().and_then(|x| base(&String::from_utf8_lossy(&x))) {
            names.insert(b);
        }
    }
    names
}

pub fn newly_started(prev: &HashSet<String>, now: &HashSet<String>, watch: &[String]) -> Option<String> {
    watch.iter().find(|w| now.contains(w.as_str()) && !prev.contains(w.as_str())).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(v: &[&str]) -> HashSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn fires_only_on_absent_to_present() {
        let watch = vec!["zoom".to_string()];
        assert_eq!(newly_started(&set(&[]), &set(&["zoom"]), &watch), Some("zoom".into()));
        assert_eq!(newly_started(&set(&["zoom"]), &set(&["zoom"]), &watch), None);
        assert_eq!(newly_started(&set(&["zoom"]), &set(&[]), &watch), None);
        assert_eq!(newly_started(&set(&[]), &set(&["discord"]), &watch), None);
    }

    #[test]
    fn basename_from_exe_and_cmdline() {
        assert_eq!(base("/usr/bin/gnome-calculator"), Some("gnome-calculator".into()));
        assert_eq!(base("/opt/zoom/zoom\0--url=x\0"), Some("zoom".into()));
        assert_eq!(base(""), None);
    }

    #[test]
    fn parses_lock_signal() {
        assert_eq!(lock_signal("/org/gnome/ScreenSaver: org.gnome.ScreenSaver.ActiveChanged (true,)"), Some(true));
        assert_eq!(lock_signal("/org/gnome/ScreenSaver: org.gnome.ScreenSaver.ActiveChanged (false,)"), Some(false));
        assert_eq!(lock_signal("/org/gnome/ScreenSaver: org.gnome.ScreenSaver.WakeUpScreen ()"), None);
    }
}
