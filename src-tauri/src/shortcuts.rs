use std::process::Command;
#[cfg(target_os = "linux")]
pub use portal::{portal_available, Portal};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Backend {
    X11,
    Portal,
    Gnome,
}

impl Backend {
    pub fn name(self) -> &'static str {
        match self {
            Backend::X11 => "x11",
            Backend::Portal => "portal",
            Backend::Gnome => "gnome",
        }
    }

    pub fn has_release(self) -> bool {
        self != Backend::Gnome
    }
}

pub fn wayland() -> bool {
    std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v == "wayland") || std::env::var_os("WAYLAND_DISPLAY").is_some()
}

pub fn detect() -> Backend {
    match std::env::var("MUTTEI_SHORTCUTS").as_deref() {
        Ok("x11") => return Backend::X11,
        Ok("portal") => return Backend::Portal,
        Ok("gnome") => return Backend::Gnome,
        _ => {}
    }
    if !wayland() {
        Backend::X11
    } else if portal_available() {
        Backend::Portal
    } else {
        Backend::Gnome
    }
}

fn split(combo: &str) -> (Vec<&str>, &str) {
    let mut parts: Vec<&str> = combo.split('+').collect();
    let key = parts.pop().unwrap_or("");
    (parts, key)
}

fn keysym(key: &str) -> String {
    let named = match key {
        "Space" => "space",
        "Comma" => "comma",
        "Period" => "period",
        "Slash" => "slash",
        "Semicolon" => "semicolon",
        "Quote" => "apostrophe",
        "BracketLeft" => "bracketleft",
        "BracketRight" => "bracketright",
        "Minus" => "minus",
        "Equal" => "equal",
        "Backquote" => "grave",
        "Backslash" => "backslash",
        "PageUp" => "Page_Up",
        "PageDown" => "Page_Down",
        "Enter" => "Return",
        _ => "",
    };
    if !named.is_empty() {
        return named.into();
    }
    if let Some(d) = key.strip_prefix("Num") {
        return format!("KP_{d}");
    }
    if key.len() == 1 {
        return key.to_lowercase();
    }
    key.into()
}

pub fn gnome_accel(combo: &str) -> String {
    let (mods, key) = split(combo);
    let mods: String = mods
        .iter()
        .map(|m| match *m {
            "Ctrl" => "<Control>",
            "Alt" => "<Alt>",
            "Shift" => "<Shift>",
            "Super" => "<Super>",
            _ => "",
        })
        .collect();
    mods + &keysym(key)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn portal_trigger(combo: &str) -> String {
    let (mods, key) = split(combo);
    let mut out: Vec<String> = mods
        .iter()
        .map(|m| match *m {
            "Ctrl" => "CTRL",
            "Alt" => "ALT",
            "Shift" => "SHIFT",
            "Super" => "LOGO",
            other => other,
        })
        .map(String::from)
        .collect();
    out.push(keysym(key));
    out.join("+")
}

const SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys";
const BASE: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/";

pub fn parse_list(out: &str) -> Vec<String> {
    out.split('\'').skip(1).step_by(2).map(String::from).collect()
}

fn gvariant_str(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn gsettings(args: &[&str]) -> Result<String, String> {
    let out = Command::new("gsettings").args(args).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Replaces Muttei's entries in GNOME's custom keybindings, keeping the user's own.
pub fn gnome_apply(bindings: &[(String, String, String)], exe: &str) -> Result<(), String> {
    let current = gsettings(&["get", SCHEMA, "custom-keybindings"])?;
    let mut paths: Vec<String> = parse_list(&current).into_iter().filter(|p| !p.contains("/muttei-")).collect();
    for (action, label, combo) in bindings {
        let path = format!("{BASE}muttei-{}/", action.replace('_', "-"));
        let schema = format!("{SCHEMA}.custom-keybinding:{path}");
        gsettings(&["set", &schema, "name", &gvariant_str(&format!("Muttei: {label}"))])?;
        gsettings(&["set", &schema, "command", &gvariant_str(&format!("'{exe}' --action {action}"))])?;
        gsettings(&["set", &schema, "binding", &gvariant_str(&gnome_accel(combo))])?;
        paths.push(path);
    }
    let list = format!("[{}]", paths.iter().map(|p| gvariant_str(p)).collect::<Vec<_>>().join(", "));
    gsettings(&["set", SCHEMA, "custom-keybindings", &list]).map(|_| ())
}

pub fn gnome_has_ours() -> bool {
    gsettings(&["get", SCHEMA, "custom-keybindings"]).is_ok_and(|s| s.contains("/muttei-"))
}

#[cfg(not(target_os = "linux"))]
fn portal_available() -> bool {
    false
}

#[cfg(not(target_os = "linux"))]
pub struct Portal;

#[cfg(not(target_os = "linux"))]
impl Portal {
    pub fn start(_: impl Fn(&str, bool) + Send + Sync + 'static) -> Result<std::sync::Arc<Self>, String> {
        Err("sem portal fora do Linux".into())
    }

    pub fn bind(&self, _: &[(String, String, String)]) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod portal {
    use super::portal_trigger;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use zbus::blocking::{Connection, Proxy};
    use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

    // MUTTEI_PORTAL_BUS lets the tests point at a fake portal
    fn portal_bus() -> String {
        std::env::var("MUTTEI_PORTAL_BUS").unwrap_or_else(|_| "org.freedesktop.portal.Desktop".into())
    }

    const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
    const GS_IFACE: &str = "org.freedesktop.portal.GlobalShortcuts";

    fn gs_proxy(conn: &Connection) -> zbus::Result<Proxy<'static>> {
        Proxy::new(conn, portal_bus(), PORTAL_PATH, GS_IFACE)
    }

    pub fn portal_available() -> bool {
        Connection::session().ok().and_then(|c| gs_proxy(&c).ok()).and_then(|p| p.get_property::<u32>("version").ok()).is_some()
    }

    type Results = HashMap<String, OwnedValue>;

    fn request(conn: &Connection, token: &str, call: impl FnOnce() -> zbus::Result<()>) -> Result<Results, String> {
        let sender = conn.unique_name().ok_or("sem nome no D-Bus")?.trim_start_matches(':').replace('.', "_");
        let path = format!("{PORTAL_PATH}/request/{sender}/{token}");
        let req = Proxy::new(conn, portal_bus(), path, "org.freedesktop.portal.Request").map_err(|e| e.to_string())?;
        let mut responses = req.receive_signal("Response").map_err(|e| e.to_string())?;
        call().map_err(|e| e.to_string())?;
        let msg = responses.next().ok_or("o portal não respondeu")?;
        let (code, results): (u32, Results) = msg.body().deserialize().map_err(|e| e.to_string())?;
        match code {
            0 => Ok(results),
            1 => Err("Você recusou os atalhos no diálogo do sistema.".into()),
            _ => Err("O portal de atalhos recusou o pedido.".into()),
        }
    }

    pub struct Portal {
        conn: Connection,
        session: Arc<Mutex<Option<OwnedObjectPath>>>,
        counter: Mutex<u32>,
    }

    impl Portal {
        pub fn start(on_key: impl Fn(&str, bool) + Send + Sync + 'static) -> Result<Arc<Self>, String> {
            let conn = Connection::session().map_err(|e| e.to_string())?;
            // GNOME identifies host apps by the desktop file; best effort, other portals don't need it
            if let Ok(reg) = Proxy::new(&conn, portal_bus(), PORTAL_PATH, "org.freedesktop.host.portal.Registry") {
                let _ = reg.call_method("Register", &("Muttei", HashMap::<&str, Value>::new()));
            }
            let p = Arc::new(Self { conn, session: Arc::new(Mutex::new(None)), counter: Mutex::new(0) });
            let on_key = Arc::new(on_key);
            for (signal, pressed) in [("Activated", true), ("Deactivated", false)] {
                let proxy = gs_proxy(&p.conn).map_err(|e| e.to_string())?;
                let stream = proxy.receive_signal(signal).map_err(|e| e.to_string())?;
                let (session, on_key) = (p.session.clone(), on_key.clone());
                std::thread::spawn(move || {
                    for msg in stream {
                        let Ok((s, id, _, _)) = msg.body().deserialize::<(OwnedObjectPath, String, u64, Results)>() else { continue };
                        if session.lock().unwrap().as_ref() == Some(&s) {
                            on_key(&id, pressed);
                        }
                    }
                });
            }
            Ok(p)
        }

        fn token(&self) -> String {
            let mut n = self.counter.lock().unwrap();
            *n += 1;
            format!("muttei{}_{n}", std::process::id())
        }

        fn close(&self) {
            if let Some(s) = self.session.lock().unwrap().take() {
                if let Ok(p) = Proxy::new(&self.conn, portal_bus(), s, "org.freedesktop.portal.Session") {
                    let _ = p.call_method("Close", &());
                }
            }
        }

        /// Rebinding needs a fresh session: version 1 of the portal can't change a bound shortcut.
        pub fn bind(&self, bindings: &[(String, String, String)]) -> Result<(), String> {
            self.close();
            if bindings.is_empty() {
                return Ok(());
            }
            let gs = gs_proxy(&self.conn).map_err(|e| e.to_string())?;
            let (t1, t2) = (self.token(), self.token());
            let created = request(&self.conn, &t1, || {
                let opts: HashMap<&str, Value> = HashMap::from([("handle_token", Value::from(t1.as_str())), ("session_handle_token", Value::from(t2.as_str()))]);
                gs.call_method("CreateSession", &(opts,)).map(|_| ())
            })?;
            let handle: String = created
                .get("session_handle")
                .and_then(|v| String::try_from(v.try_clone().ok()?).ok())
                .ok_or("o portal não criou a sessão")?;
            let session = OwnedObjectPath::try_from(handle).map_err(|e| e.to_string())?;
            let list: Vec<(String, HashMap<&str, Value>)> = bindings
                .iter()
                .map(|(a, label, combo)| {
                    (a.clone(), HashMap::from([("description", Value::from(label.clone())), ("preferred_trigger", Value::from(portal_trigger(combo)))]))
                })
                .collect();
            let t3 = self.token();
            request(&self.conn, &t3, || {
                let opts: HashMap<&str, Value> = HashMap::from([("handle_token", Value::from(t3.as_str()))]);
                gs.call_method("BindShortcuts", &(&session, list, "", opts)).map(|_| ())
            })?;
            *self.session.lock().unwrap() = Some(session);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_to_gnome_accels() {
        assert_eq!(gnome_accel("Ctrl+Shift+M"), "<Control><Shift>m");
        assert_eq!(gnome_accel("Ctrl+Alt+Space"), "<Control><Alt>space");
        assert_eq!(gnome_accel("Super+F5"), "<Super>F5");
        assert_eq!(gnome_accel("Ctrl+Alt+Comma"), "<Control><Alt>comma");
        assert_eq!(gnome_accel("Ctrl+Num1"), "<Control>KP_1");
        assert_eq!(gnome_accel("Ctrl+PageUp"), "<Control>Page_Up");
    }

    #[test]
    fn converts_to_portal_triggers() {
        assert_eq!(portal_trigger("Ctrl+Shift+M"), "CTRL+SHIFT+m");
        assert_eq!(portal_trigger("Super+Alt+Space"), "LOGO+ALT+space");
    }

    #[test]
    fn parses_gsettings_lists() {
        assert_eq!(parse_list("@as []\n"), Vec::<String>::new());
        assert_eq!(parse_list("['/a/custom0/', '/a/muttei-ptt/']\n"), vec!["/a/custom0/", "/a/muttei-ptt/"]);
    }

    #[test]
    fn quotes_gvariant_strings() {
        assert_eq!(gvariant_str("it's"), "'it\\'s'");
    }

    #[test]
    fn only_gnome_lacks_release() {
        assert!(Backend::X11.has_release() && Backend::Portal.has_release() && !Backend::Gnome.has_release());
    }
}
