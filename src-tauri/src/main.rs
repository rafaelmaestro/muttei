mod audio;
mod remote;
mod rules;
mod settings;

use remote::{Iface, PairResult, Pairing};
use serde::Serialize;
use settings::{Browser, Device, Settings, DEFAULT_SHORTCUT, TOGGLE_MUTE};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tokio::sync::{broadcast, oneshot};

const TRAY_ON: &[u8] = include_bytes!("../icons/tray.png");
const TRAY_MUTED: &[u8] = include_bytes!("../icons/tray-muted.png");
const PILL_W: f64 = 320.0;
const PILL_H: f64 = 76.0;
const DOT: f64 = 44.0;
const TOP: f64 = 40.0;

#[derive(Clone)]
pub enum Event {
    Muted(bool),
    Devices,
}

struct Tray {
    icon: TrayIcon,
    status: MenuItem<tauri::Wry>,
    toggle: MenuItem<tauri::Wry>,
}

pub struct Core {
    app: AppHandle,
    s: Mutex<Settings>,
    muted: AtomicBool,
    audio_lock: Mutex<()>,
    locked_mute: AtomicBool,
    pairing: Mutex<Pairing>,
    phone_stop: Mutex<Option<oneshot::Sender<()>>>,
    phone_addr: Mutex<Option<SocketAddr>>,
    phone_err: Mutex<String>,
    pub tx: broadcast::Sender<Event>,
    live: Mutex<HashMap<String, u32>>,
    pair_until: Mutex<Option<Instant>>,
    sc_err: Mutex<String>,
    last: Mutex<String>,
    running: Mutex<HashSet<String>>,
    tray: Mutex<Option<Tray>>,
}

#[derive(Serialize, Clone)]
struct PublicDevice {
    id: String,
    name: String,
    when: String,
}

#[derive(Serialize, Clone)]
struct Snapshot {
    muted: bool,
    shortcut: String,
    sound: bool,
    pill: bool,
    dot: bool,
    dot_corner: String,
    autostart: bool,
    lock_rule: bool,
    app_rule: bool,
    apps: Vec<String>,
    running: Vec<String>,
    phone_on: bool,
    port: u16,
    iface: Option<String>,
    ifaces: Vec<Iface>,
    hidden: String,
    ip: Option<String>,
    url: Option<String>,
    qr: String,
    code: String,
    fails: u8,
    pair_locked: u64,
    devices: Vec<PublicDevice>,
    phone_err: String,
    ufw: bool,
    browsers: Vec<(Browser, bool)>,
    pair_left: u64,
    sc_err: String,
    last: String,
    intro_seen: bool,
}

fn now_label() -> String {
    std::process::Command::new("date")
        .arg("+%d/%m")
        .output()
        .map(|o| format!("pareado em {}", String::from_utf8_lossy(&o.stdout).trim()))
        .unwrap_or_else(|_| "pareado agora".into())
}

const TAKEN: &[(&str, &str)] = &[
    ("Ctrl+Space", "Ctrl+Space já está em uso pelo IBus (troca de layout do teclado). Escolha outro atalho."),
    ("Super+L", "Super+L é do GNOME (travar a tela)."),
    ("Ctrl+Alt+T", "Ctrl+Alt+T já abre o Terminal no GNOME."),
];

impl Core {
    pub fn muted(&self) -> bool {
        self.muted.load(Ordering::SeqCst)
    }

    pub fn hostname(&self) -> String {
        std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string()).unwrap_or_else(|_| "este PC".into())
    }

    fn settings(&self) -> Settings {
        self.s.lock().unwrap().clone()
    }

    fn snapshot(&self) -> Snapshot {
        let s = self.settings();
        let (ifaces, hidden) = remote::interfaces();
        let chosen = remote::pick(&ifaces, &s.iface);
        let ip = chosen.as_ref().and_then(|i| i.ip.clone());
        let url = ip.as_ref().map(|ip| format!("http://{ip}:{}/", s.port));
        let p = self.pairing.lock().unwrap();
        let live = self.live.lock().unwrap();
        let left = self.pair_until.lock().unwrap().map(|t| t.saturating_duration_since(Instant::now()).as_secs()).unwrap_or(0);
        Snapshot {
            muted: self.muted(),
            shortcut: s.shortcut(),
            sound: s.sound,
            pill: s.pill,
            dot: s.dot,
            dot_corner: s.dot_corner.clone(),
            autostart: s.autostart,
            lock_rule: s.lock_rule,
            app_rule: s.app_rule,
            apps: s.apps.clone(),
            running: self.running.lock().unwrap().iter().filter(|r| s.apps.contains(r)).cloned().collect(),
            phone_on: s.phone_on,
            port: s.port,
            iface: chosen.map(|i| i.name),
            ifaces,
            hidden,
            qr: url.as_deref().map(remote::qr_svg).unwrap_or_default(),
            ip,
            url,
            code: p.code.clone(),
            fails: p.fails,
            pair_locked: p.locked_secs(),
            devices: s.devices.iter().map(|d| PublicDevice { id: d.id.clone(), name: d.name.clone(), when: d.when.clone() }).collect(),
            phone_err: self.phone_err.lock().unwrap().clone(),
            ufw: remote::ufw_enabled(),
            browsers: s.meet_browsers.iter().map(|b| (b.clone(), live.get(&b.id).copied().unwrap_or(0) > 0)).collect(),
            pair_left: left,
            sc_err: self.sc_err.lock().unwrap().clone(),
            last: self.last.lock().unwrap().clone(),
            intro_seen: s.intro_seen,
        }
    }

    fn emit_state(&self) {
        let _ = self.app.emit("state", self.snapshot());
    }

    fn toast(&self, msg: &str) {
        let _ = self.app.emit_to("settings", "toast", msg);
    }

    fn pill(&self, kind: &str, text: &str, sub: &str) {
        if kind != "warn" && !self.s.lock().unwrap().pill {
            return;
        }
        let payload = serde_json::json!({"kind": kind, "text": text, "sub": sub});
        for label in place_pills(&self.app) {
            let _ = self.app.emit_to(label.as_str(), "pill", payload.clone());
        }
    }

    fn effects(&self, m: bool, src: &str, sound: bool) {
        if sound && self.s.lock().unwrap().sound {
            audio::play(m);
        }
        self.pill(if m { "off" } else { "on" }, if m { "Mudo" } else { "No ar" }, src);
        self.update_indicators();
        let _ = self.tx.send(Event::Muted(m));
        self.emit_state();
    }

    fn update_indicators(&self) {
        let m = self.muted();
        let s = self.settings();
        if let Some(t) = self.tray.lock().unwrap().as_ref() {
            let _ = t.icon.set_icon(Image::from_bytes(if m { TRAY_MUTED } else { TRAY_ON }).ok());
            let _ = t.status.set_text(if m { "Microfone mudo" } else { "Microfone no ar" });
            let _ = t.toggle.set_text(format!("{}    {}", if m { "Desmutar" } else { "Mutar" }, s.shortcut()));
        }
    }

    pub fn set_muted(&self, m: bool, src: &str) -> bool {
        // one lock around wpctl + store so the 500ms poll can't replay a stale state over this change
        let ok = {
            let _g = self.audio_lock.lock().unwrap();
            if m == self.muted() {
                return m;
            }
            let ok = audio::set_muted(m);
            if ok {
                self.muted.store(m, Ordering::SeqCst);
            }
            ok
        };
        if !ok {
            self.pill("warn", "O microfone não mudou", "wpctl falhou");
            return self.muted();
        }
        *self.last.lock().unwrap() = audio::command_text(m);
        self.effects(m, src, true);
        m
    }

    pub fn toggle(&self, src: &str) -> bool {
        self.set_muted(!self.muted(), src)
    }

    pub fn pair(&self, code: &str, name: &str) -> Result<String, PairResult> {
        let result = self.pairing.lock().unwrap().try_code(code);
        match result {
            PairResult::Ok => {}
            PairResult::Locked { .. } => return Err(result),
            other => {
                self.emit_state();
                return Err(other);
            }
        }
        let token = remote::token();
        {
            let mut s = self.s.lock().unwrap();
            s.devices.push(Device { id: token[..8].to_string(), name: name.into(), when: now_label(), token: token.clone() });
            s.save();
        }
        self.toast("Celular pareado. O código foi trocado.");
        self.emit_state();
        Ok(token)
    }

    pub fn valid_token(&self, t: &str) -> bool {
        !t.is_empty() && self.s.lock().unwrap().devices.iter().any(|d| d.token == t)
    }

    fn revoke(&self, id: &str) {
        let name = {
            let mut s = self.s.lock().unwrap();
            let name = s.devices.iter().find(|d| d.id == id).map(|d| d.name.clone()).unwrap_or_else(|| "Dispositivo".into());
            s.devices.retain(|d| d.id != id);
            s.save();
            name
        };
        let _ = self.tx.send(Event::Devices);
        self.toast(&format!("{name} revogado."));
        self.emit_state();
    }

    pub fn meet_approve(&self, id: &str, name: &str) -> bool {
        let mut s = self.s.lock().unwrap();
        if let Some(b) = s.meet_browsers.iter_mut().find(|b| b.id == id) {
            b.name = name.into();
            s.save();
            return true;
        }
        let mut until = self.pair_until.lock().unwrap();
        let open = until.is_some_and(|t| t > Instant::now());
        if !s.meet_browsers.is_empty() && !open {
            return false;
        }
        s.meet_browsers.push(Browser { id: id.into(), name: name.into() });
        s.save();
        *until = None;
        drop((s, until));
        self.toast(&format!("{name} conectado ao Muttei."));
        true
    }

    pub fn meet_live(&self, id: &str, on: bool) {
        {
            let mut l = self.live.lock().unwrap();
            let n = l.entry(id.to_string()).or_insert(0);
            *n = if on { *n + 1 } else { n.saturating_sub(1) };
        }
        self.emit_state();
    }

    fn restart_phone(self: &Arc<Self>) {
        if let Some(stop) = self.phone_stop.lock().unwrap().take() {
            let _ = stop.send(());
        }
        *self.phone_addr.lock().unwrap() = None;
        *self.phone_err.lock().unwrap() = String::new();
        let s = self.settings();
        if !s.phone_on {
            return;
        }
        let (ifaces, _) = remote::interfaces();
        let Some(ip) = remote::pick(&ifaces, &s.iface).and_then(|i| i.ip) else {
            *self.phone_err.lock().unwrap() = "Nenhuma rede conectada. Conecte a Wi-Fi ou escolha outra interface.".into();
            return;
        };
        let Ok(addr) = format!("{ip}:{}", s.port).parse::<SocketAddr>() else { return };
        let (tx, rx) = oneshot::channel();
        *self.phone_stop.lock().unwrap() = Some(tx);
        *self.phone_addr.lock().unwrap() = Some(addr);
        let core = self.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = remote::serve_phone(core.clone(), addr, rx).await {
                *core.phone_err.lock().unwrap() = format!("Não consegui abrir {addr}: {e}");
                *core.phone_addr.lock().unwrap() = None;
                core.emit_state();
            }
        });
    }

    fn phone_wants_rebind(&self) -> bool {
        let s = self.settings();
        if !s.phone_on {
            return false;
        }
        let (ifaces, _) = remote::interfaces();
        let want = remote::pick(&ifaces, &s.iface).and_then(|i| format!("{}:{}", i.ip?, s.port).parse::<SocketAddr>().ok());
        want.is_some() && want != *self.phone_addr.lock().unwrap()
    }

    fn register(self: &Arc<Self>, combo: &str) -> Result<(), String> {
        let core = self.clone();
        self.app
            .global_shortcut()
            .on_shortcut(combo, move |_, _, e| {
                if e.state == ShortcutState::Pressed {
                    core.toggle("atalho");
                }
            })
            .map_err(|e| e.to_string())
    }

    fn register_current(self: &Arc<Self>) {
        let combo = self.settings().shortcut();
        let err = self.register(&combo).err().map(|e| format!("Não consegui registrar {combo}: {e}. Outro programa pode estar usando esse atalho."));
        *self.sc_err.lock().unwrap() = err.unwrap_or_default();
    }

    fn set_shortcut(self: &Arc<Self>, combo: &str) -> Result<(), String> {
        let fail = |e: String| {
            *self.sc_err.lock().unwrap() = e.clone();
            self.register_current();
            *self.sc_err.lock().unwrap() = e.clone();
            self.emit_state();
            Err(e)
        };
        if let Some((_, why)) = TAKEN.iter().find(|(k, _)| *k == combo) {
            return fail(why.to_string());
        }
        if !combo.contains('+') {
            return fail("Use pelo menos um modificador (Ctrl, Alt, Shift ou Super).".into());
        }
        let _ = self.app.global_shortcut().unregister_all();
        if let Err(e) = self.register(combo) {
            return fail(format!("Não consegui registrar {combo}: {e}. Outro programa pode estar usando esse atalho."));
        }
        {
            let mut s = self.s.lock().unwrap();
            s.shortcuts.insert(TOGGLE_MUTE.into(), combo.into());
            s.save();
        }
        *self.sc_err.lock().unwrap() = String::new();
        self.update_indicators();
        self.toast(&format!("Atalho salvo: {combo}"));
        self.emit_state();
        Ok(())
    }

    fn update(self: &Arc<Self>, patch: serde_json::Value) -> Result<(), String> {
        let (old, new) = {
            let mut s = self.s.lock().unwrap();
            let new = s.merge(patch)?;
            let old = std::mem::replace(&mut *s, new.clone());
            s.save();
            (old, new)
        };
        if (old.phone_on, &old.iface, old.port) != (new.phone_on, &new.iface, new.port) {
            self.restart_phone();
        }
        if old.autostart != new.autostart {
            let a = self.app.autolaunch();
            let _ = if new.autostart { a.enable() } else { a.disable() };
        }
        if old.dot_corner != new.dot_corner {
            place_dot(&self.app, &new.dot_corner);
        }
        self.update_indicators();
        self.emit_state();
        Ok(())
    }
}

fn overlay(app: &AppHandle, label: &str, w: f64, h: f64, page: &str) -> tauri::Result<WebviewWindow> {
    let win = WebviewWindowBuilder::new(app, label, WebviewUrl::App(page.into()))
        .inner_size(w, h)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(true)
        .title(format!("Muttei {label}"))
        .min_inner_size(w, h)
        .max_inner_size(w, h)
        .build()?;
    let _ = win.set_ignore_cursor_events(true);
    Ok(win)
}

fn place_pills(app: &AppHandle) -> Vec<String> {
    let monitors = app.available_monitors().unwrap_or_default();
    let mut labels = vec![];
    for (i, m) in monitors.iter().enumerate() {
        let label = format!("pill-{i}");
        let w = match app.get_webview_window(&label) {
            Some(w) => w,
            None => match overlay(app, &label, PILL_W, PILL_H, "pill.html") {
                Ok(w) => w,
                Err(_) => continue,
            },
        };
        let k = m.scale_factor();
        let x = m.position().x as f64 + (m.size().width as f64 - PILL_W * k) / 2.0;
        let y = m.position().y as f64 + TOP * k;
        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
        labels.push(label);
    }
    for (label, w) in app.webview_windows() {
        if label.starts_with("pill-") && !labels.contains(&label) {
            let _ = w.close();
        }
    }
    labels
}

fn place_dot(app: &AppHandle, corner: &str) {
    let Some(w) = app.get_webview_window("dot") else { return };
    let Some(m) = app.primary_monitor().ok().flatten() else { return };
    // WebKitGTK enforces a ~200px minimum, so anchor the real window size and let CSS pin the dot to the corner
    let Ok(size) = w.outer_size() else { return };
    let (mx, my) = (m.position().x as f64, m.position().y as f64);
    let (sw, sh) = (m.size().width as f64, m.size().height as f64);
    let (ww, wh) = (size.width as f64, size.height as f64);
    let top = my + (TOP - 12.0) * m.scale_factor();
    let (x, y) = match corner {
        "tl" => (mx, top),
        "br" => (mx + sw - ww, my + sh - wh),
        "bl" => (mx, my + sh - wh),
        _ => (mx + sw - ww, top),
    };
    let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
}

fn show_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

type C<'a> = State<'a, Arc<Core>>;

#[tauri::command]
fn get_state(core: C) -> Snapshot {
    core.snapshot()
}
#[tauri::command]
fn toggle_mute(core: C) -> bool {
    core.toggle("atalho")
}
#[tauri::command]
fn update(core: C, patch: serde_json::Value) -> Result<(), String> {
    core.update(patch)
}
#[tauri::command]
fn rec_start(core: C) {
    let _ = core.app.global_shortcut().unregister_all();
}
#[tauri::command]
fn rec_cancel(core: C) {
    core.register_current();
    core.emit_state();
}
#[tauri::command]
fn set_shortcut(core: C, combo: String) -> Result<(), String> {
    core.set_shortcut(&combo)
}
#[tauri::command]
fn reset_shortcut(core: C) -> Result<(), String> {
    core.set_shortcut(DEFAULT_SHORTCUT)
}
#[tauri::command]
fn regen_code(core: C) {
    core.pairing.lock().unwrap().regen();
    core.emit_state();
}
#[tauri::command]
fn revoke(core: C, id: String) {
    core.revoke(&id);
}
#[tauri::command]
fn pair_window(core: C, open: bool) {
    *core.pair_until.lock().unwrap() = open.then(|| Instant::now() + Duration::from_secs(120));
    core.emit_state();
}
#[tauri::command]
fn test_sound() {
    audio::play(true);
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(450));
        audio::play(false);
    });
}
#[tauri::command]
fn lock_now() {
    let _ = std::process::Command::new("loginctl").arg("lock-session").spawn();
}
#[tauri::command]
fn dot_ready(core: C) {
    place_dot(&core.app, &core.settings().dot_corner);
}
#[tauri::command]
fn hide_settings(core: C) {
    if let Some(w) = core.app.get_webview_window("settings") {
        let _ = w.hide();
    }
}

fn setup(app: &AppHandle) -> tauri::Result<()> {
    let s = Settings::load();
    let first_run = !s.intro_seen;
    let (tx, _) = broadcast::channel(32);
    let core = Arc::new(Core {
        app: app.clone(),
        muted: AtomicBool::new(audio::get_muted().unwrap_or(false)),
        audio_lock: Mutex::new(()),
        locked_mute: AtomicBool::new(false),
        s: Mutex::new(s.clone()),
        pairing: Mutex::new(Pairing::new()),
        phone_stop: Mutex::new(None),
        phone_addr: Mutex::new(None),
        phone_err: Mutex::new(String::new()),
        tx,
        live: Mutex::new(HashMap::new()),
        pair_until: Mutex::new(None),
        sc_err: Mutex::new(String::new()),
        last: Mutex::new("wpctl get-volume @DEFAULT_AUDIO_SOURCE@".into()),
        running: Mutex::new(HashSet::new()),
        tray: Mutex::new(None),
    });
    app.manage(core.clone());

    WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("Ajustes do Muttei")
        .inner_size(820.0, 640.0)
        .min_inner_size(560.0, 480.0)
        .decorations(false)
        .transparent(true)
        .visible(first_run)
        .build()?;
    place_pills(app);
    overlay(app, "dot", DOT, DOT, "dot.html")?;

    let status = MenuItem::with_id(app, "status", "Microfone no ar", false, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "Mutar", true, None::<&str>)?;
    let open = MenuItem::with_id(app, "settings", "Ajustes", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Sair do Muttei", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&status, &toggle, &open, &PredefinedMenuItem::separator(app)?, &quit])?;
    let c = core.clone();
    let icon = TrayIconBuilder::with_id("muttei")
        .icon(Image::from_bytes(TRAY_ON)?)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, e| match e.id.as_ref() {
            "toggle" => {
                c.toggle("tray");
            }
            "settings" => show_settings(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    *core.tray.lock().unwrap() = Some(Tray { icon, status, toggle });

    {
        let a = app.autolaunch();
        let _ = if s.autostart { a.enable() } else { a.disable() };
    }
    core.register_current();
    core.update_indicators();
    core.restart_phone();
    tauri::async_runtime::spawn(remote::serve_meet(core.clone()));

    let c = core.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(500));
        // ponytail: polling wpctl; switch to pw-mon events if it ever shows up in CPU
        let changed = {
            let _g = c.audio_lock.lock().unwrap();
            match audio::get_muted() {
                Some(m) if m != c.muted() => {
                    c.muted.store(m, Ordering::SeqCst);
                    Some(m)
                }
                _ => None,
            }
        };
        if let Some(m) = changed {
            c.effects(m, "sistema", false);
        }
    });

    let c = core.clone();
    rules::watch_lock(move |locked| {
        let s = c.settings();
        if locked && s.lock_rule && !c.muted() {
            let m = c.set_muted(true, "regra: tela travada");
            c.locked_mute.store(m, Ordering::SeqCst);
        } else if !locked && c.locked_mute.swap(false, Ordering::SeqCst) && c.muted() {
            c.pill("off", "Mudo", "regra: tela travada");
        }
    });

    let c = core.clone();
    std::thread::spawn(move || {
        let mut prev = rules::running();
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let now = rules::running();
            let s = c.settings();
            if s.app_rule && !c.muted() {
                if let Some(name) = rules::newly_started(&prev, &now, &s.apps) {
                    c.set_muted(true, &format!("regra: {name} abriu"));
                }
            }
            let changed = s.apps.iter().any(|a| prev.contains(a) != now.contains(a));
            *c.running.lock().unwrap() = now.clone();
            if changed {
                c.emit_state();
            }
            prev = now;
        }
    });

    let c = core.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(5));
        if c.phone_wants_rebind() {
            c.restart_phone();
            c.emit_state();
        }
    });

    let c = core.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        let mut until = c.pair_until.lock().unwrap();
        if let Some(t) = *until {
            if t <= Instant::now() {
                *until = None;
            }
            drop(until);
            c.emit_state();
        }
    });
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show_settings(app)))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .invoke_handler(tauri::generate_handler![
            get_state, toggle_mute, update, rec_start, rec_cancel, set_shortcut, reset_shortcut, regen_code, revoke, pair_window, test_sound, lock_now, hide_settings, dot_ready
        ])
        .setup(|app| Ok(setup(app.handle())?))
        .build(tauri::generate_context!())
        .expect("muttei failed to start")
        .run(|_, e| {
            if let tauri::RunEvent::ExitRequested { api, code: None, .. } = e {
                api.prevent_exit();
            }
        });
}
