mod audio;
mod ptt;
mod remote;
mod rules;
mod settings;
mod shortcuts;

use remote::{Iface, PairResult, Pairing};
use serde::Serialize;
use settings::{Browser, Device, Settings, DEFAULT_SHORTCUT, TOGGLE_MUTE};
use std::collections::BTreeMap;
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
    Cmd(String),
    Meet,
}

pub const MEET_CMDS: &[&str] = &["camera", "hand", "leave"];

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
pub struct MeetStatus {
    pub in_call: bool,
    pub cam: Option<bool>,
    pub hand: bool,
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
    sc_errs: Mutex<BTreeMap<String, String>>,
    last: Mutex<String>,
    mics: Mutex<Vec<audio::Mic>>,
    target: Mutex<String>,
    running: Mutex<HashSet<String>>,
    tray: Mutex<Option<Tray>>,
    hold: Mutex<ptt::Hold>,
    meet: Mutex<Option<MeetStatus>>,
    cmd_src: Mutex<String>,
    backend: shortcuts::Backend,
    portal: Mutex<Option<Arc<shortcuts::Portal>>>,
}

#[derive(Serialize, Clone)]
struct PublicDevice {
    id: String,
    name: String,
    when: String,
}

#[derive(Serialize, Clone)]
struct HoldView {
    kind: String,
    prior: bool,
}

#[derive(Serialize, Clone)]
struct Snapshot {
    muted: bool,
    holding: Option<HoldView>,
    shortcut: String,
    shortcuts: BTreeMap<String, String>,
    shortcut_backend: &'static str,
    sound: bool,
    pill: bool,
    dot: bool,
    dot_corner: String,
    dot_style: String,
    dot_anim: String,
    autostart: bool,
    lock_rule: bool,
    app_rule: bool,
    meet_rule: bool,
    mic: Option<String>,
    mic_desc: String,
    mics: Vec<audio::Mic>,
    mic_gone: bool,
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
    sc_errs: BTreeMap<String, String>,
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

impl Core {
    pub fn muted(&self) -> bool {
        self.muted.load(Ordering::SeqCst)
    }

    fn target(&self) -> String {
        self.target.lock().unwrap().clone()
    }

    fn refresh_mics(&self) {
        let mics = audio::mics();
        let target = audio::target(&mics, self.settings().mic.as_deref());
        let retarget = *self.target.lock().unwrap() != target;
        let relist = *self.mics.lock().unwrap() != mics;
        *self.mics.lock().unwrap() = mics;
        if retarget {
            let m = {
                let _g = self.audio_lock.lock().unwrap();
                *self.target.lock().unwrap() = target.clone();
                audio::get_muted(&target).unwrap_or(false)
            };
            self.muted.store(m, Ordering::SeqCst);
            *self.last.lock().unwrap() = format!("wpctl get-volume {target}");
            self.update_indicators();
        }
        if retarget || relist {
            self.emit_state();
        }
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
            holding: {
                let h = self.hold.lock().unwrap();
                h.kind.zip(h.prior).map(|(k, p)| HoldView { kind: k.into(), prior: p })
            },
            shortcut: s.shortcut(),
            shortcuts: s.shortcuts.clone(),
            shortcut_backend: self.backend.name(),
            sound: s.sound,
            pill: s.pill,
            dot: s.dot,
            dot_corner: s.dot_corner.clone(),
            dot_style: s.dot_style.clone(),
            dot_anim: s.dot_anim.clone(),
            autostart: s.autostart,
            lock_rule: s.lock_rule,
            app_rule: s.app_rule,
            meet_rule: s.meet_rule,
            mic_gone: s.mic.is_some() && *self.target.lock().unwrap() == audio::DEFAULT,
            mic: s.mic.clone(),
            mic_desc: s.mic_desc.clone(),
            mics: self.mics.lock().unwrap().clone(),
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
            sc_errs: self.sc_errs.lock().unwrap().clone(),
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
        self.pill_icon(kind, text, sub, "");
    }

    fn pill_icon(&self, kind: &str, text: &str, sub: &str, icon: &str) {
        if kind != "warn" && !self.s.lock().unwrap().pill {
            return;
        }
        let payload = serde_json::json!({"kind": kind, "text": text, "sub": sub, "icon": icon});
        for label in place_pills(&self.app) {
            let _ = self.app.emit_to(label.as_str(), "pill", payload.clone());
        }
    }

    fn effects(&self, m: bool, src: &str, sound: bool) {
        ensure_dots(&self.app, &self.settings().dot_corner);
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
            let ok = audio::set_muted(&self.target(), m);
            if ok {
                self.muted.store(m, Ordering::SeqCst);
            }
            ok
        };
        if !ok {
            self.pill("warn", "O microfone não mudou", "wpctl falhou");
            return self.muted();
        }
        *self.last.lock().unwrap() = audio::command_text(&self.target(), m);
        self.effects(m, src, true);
        m
    }

    pub fn toggle(&self, src: &str) -> bool {
        let restore = self.hold.lock().unwrap().toggled();
        if let Some(p) = restore {
            self.pill(if p { "off" } else { "on" }, if p { "Ao soltar: mudo" } else { "Ao soltar: no ar" }, "segurando");
            self.emit_state();
            return self.muted();
        }
        self.set_muted(!self.muted(), src)
    }

    fn hold_key(&self, kind: &'static str, state: ShortcutState) {
        if state == ShortcutState::Pressed {
            let target = self.hold.lock().unwrap().press(kind, self.muted());
            let Some(t) = target else { return };
            if t != self.muted() {
                self.set_muted(t, "segurando");
            } else {
                self.pill(if t { "off" } else { "on" }, if t { "Mudo" } else { "No ar" }, "segurando");
            }
        } else {
            let prior = self.hold.lock().unwrap().release(kind);
            let Some(p) = prior else { return };
            if p != self.muted() {
                self.set_muted(p, "soltou");
            }
        }
        self.emit_state();
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
        let any = {
            let mut l = self.live.lock().unwrap();
            let n = l.entry(id.to_string()).or_insert(0);
            *n = if on { *n + 1 } else { n.saturating_sub(1) };
            l.values().any(|n| *n > 0)
        };
        if !any {
            self.set_meet_status(None);
        }
        self.emit_state();
    }

    pub fn meet_status(&self) -> Option<MeetStatus> {
        *self.meet.lock().unwrap()
    }

    pub fn set_meet_status(&self, st: Option<MeetStatus>) {
        let (changed, joined) = {
            let mut m = self.meet.lock().unwrap();
            // only a seen pre-join screen counts, so reconnecting mid-call doesn't mute
            let joined = matches!(*m, Some(MeetStatus { in_call: false, .. })) && st.is_some_and(|s| s.in_call);
            let changed = *m != st;
            *m = st;
            (changed, joined)
        };
        if changed {
            let _ = self.tx.send(Event::Meet);
        }
        if joined && self.settings().meet_rule && !self.muted() {
            self.set_muted(true, "regra: entrou no Meet");
        }
    }

    pub fn meet_cmd(&self, cmd: &str, src: &str) -> bool {
        match self.meet_status() {
            None => self.pill("warn", "Meet não conectado", src),
            Some(st) if !st.in_call => self.pill("warn", "Botão não encontrado no Meet", src),
            Some(_) => {
                *self.cmd_src.lock().unwrap() = src.into();
                let _ = self.tx.send(Event::Cmd(cmd.into()));
                return true;
            }
        }
        false
    }

    pub fn meet_result(&self, cmd: &str, ok: bool, on: Option<bool>) {
        let src = self.cmd_src.lock().unwrap().clone();
        if !ok {
            return self.pill("warn", "Botão não encontrado no Meet", &src);
        }
        match (cmd, on) {
            ("camera", Some(on)) => self.pill_icon(if on { "on" } else { "off" }, if on { "Câmera ligada" } else { "Câmera desligada" }, &src, if on { "cam" } else { "camOff" }),
            ("hand", Some(up)) => self.pill_icon("hand", if up { "Mão levantada" } else { "Mão abaixada" }, &src, "hand"),
            ("leave", _) => self.pill_icon("off", "Saiu da chamada", &src, "leave"),
            _ => {}
        }
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

    fn on_key(&self, action: &str, state: ShortcutState) {
        match action {
            TOGGLE_MUTE if state == ShortcutState::Pressed => {
                self.toggle("atalho");
            }
            "ptt" => self.hold_key("ptt", state),
            "ptc" => self.hold_key("ptc", state),
            cmd if state == ShortcutState::Pressed && MEET_CMDS.contains(&cmd) => {
                self.meet_cmd(cmd, "atalho");
            }
            _ => {}
        }
    }

    fn bindings(&self) -> Vec<(String, String, String)> {
        self.settings()
            .shortcuts
            .into_iter()
            .filter(|(a, _)| self.backend.has_release() || !matches!(a.as_str(), "ptt" | "ptc"))
            .map(|(a, c)| {
                let label = settings::ACTIONS.iter().find(|(k, _)| *k == a).map(|(_, l)| *l).unwrap_or("").to_string();
                (a, label, c)
            })
            .collect()
    }

    fn exe() -> String {
        std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "muttei".into())
    }

    fn apply_bindings(self: &Arc<Self>, list: Vec<(String, String, String)>) {
        let fail_all = |core: &Core, e: String, list: &[(String, String, String)]| {
            *core.sc_errs.lock().unwrap() = list.iter().map(|(a, _, _)| (a.clone(), e.clone())).collect();
        };
        match self.backend {
            shortcuts::Backend::X11 => {
                let gs = self.app.global_shortcut();
                let _ = gs.unregister_all();
                let mut errs = BTreeMap::new();
                for (action, _, combo) in list {
                    let core = self.clone();
                    let a = action.clone();
                    if let Err(e) = gs.on_shortcut(combo.as_str(), move |_, _, e| core.on_key(&a, e.state)) {
                        errs.insert(action, format!("Não consegui registrar {combo}: {e}. Outro programa pode estar usando esse atalho."));
                    }
                }
                *self.sc_errs.lock().unwrap() = errs;
            }
            shortcuts::Backend::Gnome => match shortcuts::gnome_apply(&list, &Self::exe()) {
                Ok(()) => self.sc_errs.lock().unwrap().clear(),
                Err(e) => fail_all(self, format!("Não consegui registrar os atalhos no GNOME: {e}"), &list),
            },
            shortcuts::Backend::Portal => {
                let Some(portal) = self.portal.lock().unwrap().clone() else {
                    return fail_all(self, "O portal de atalhos do sistema não respondeu.".into(), &list);
                };
                let core = self.clone();
                // binding can open a system dialog and wait for the user, so it can't block the caller
                std::thread::spawn(move || {
                    match portal.bind(&list) {
                        Ok(()) => core.sc_errs.lock().unwrap().clear(),
                        Err(e) => fail_all(&core, e, &list),
                    }
                    core.emit_state();
                });
            }
        }
    }

    fn register_all(self: &Arc<Self>) {
        self.apply_bindings(self.bindings());
    }

    fn unregister_all(self: &Arc<Self>) {
        self.apply_bindings(vec![]);
    }

    fn set_shortcut(self: &Arc<Self>, action: &str, combo: &str) -> Result<(), String> {
        let old = {
            let mut s = self.s.lock().unwrap();
            if let Err(e) = s.validate_shortcut(action, combo) {
                drop(s);
                self.register_all();
                self.sc_errs.lock().unwrap().insert(action.into(), e.clone());
                self.emit_state();
                return Err(e);
            }
            let old = s.shortcuts.insert(action.into(), combo.into());
            s.save();
            old
        };
        self.register_all();
        let failed = self.sc_errs.lock().unwrap().get(action).cloned();
        if let Some(e) = failed {
            {
                let mut s = self.s.lock().unwrap();
                match old {
                    Some(o) => s.shortcuts.insert(action.into(), o),
                    None => s.shortcuts.remove(action),
                };
                s.save();
            }
            self.register_all();
            self.sc_errs.lock().unwrap().insert(action.into(), e.clone());
            self.emit_state();
            return Err(e);
        }
        self.update_indicators();
        let label = settings::ACTIONS.iter().find(|(a, _)| *a == action).map(|(_, l)| *l).unwrap_or(action);
        self.toast(&format!("Atalho salvo: {label} · {combo}"));
        self.emit_state();
        Ok(())
    }

    fn clear_shortcut(self: &Arc<Self>, action: &str) {
        if action == TOGGLE_MUTE {
            return;
        }
        {
            let mut s = self.s.lock().unwrap();
            s.shortcuts.remove(action);
            s.save();
        }
        self.register_all();
        self.emit_state();
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
        if old.mic != new.mic {
            self.refresh_mics();
        }
        if old.dot_corner != new.dot_corner {
            ensure_dots(&self.app, &new.dot_corner);
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
        .focusable(false)
        .visible(true)
        .title(format!("Muttei {label}"))
        .min_inner_size(w, h)
        .max_inner_size(w, h)
        .build()?;
    let _ = win.set_ignore_cursor_events(true);
    // GNOME flags an unfocused new window as "is ready" and shows a notification; clear it
    let _ = win.request_user_attention(None);
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

fn place_dot(app: &AppHandle, label: &str, corner: &str) {
    let Some(w) = app.get_webview_window(label) else { return };
    let i: usize = label.trim_start_matches("dot-").parse().unwrap_or(0);
    let Some(m) = app.available_monitors().ok().and_then(|ms| ms.into_iter().nth(i)) else { return };
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

fn ensure_dots(app: &AppHandle, corner: &str) {
    let n = app.available_monitors().map(|m| m.len()).unwrap_or(1);
    for i in 0..n {
        let label = format!("dot-{i}");
        if app.get_webview_window(&label).is_some() {
            place_dot(app, &label, corner);
        } else {
            let _ = overlay(app, &label, DOT, DOT, "dot.html");
        }
    }
    for (label, w) in app.webview_windows() {
        if label.strip_prefix("dot-").and_then(|i| i.parse::<usize>().ok()).is_some_and(|i| i >= n) {
            let _ = w.close();
        }
    }
}

fn action_arg(args: impl IntoIterator<Item = String>) -> Option<String> {
    let mut it = args.into_iter();
    it.find(|a| a == "--action")?;
    // --action only ever reports a press, so the hold actions would never be released
    it.next().filter(|a| settings::ACTIONS.iter().any(|(k, _)| k == a) && !matches!(a.as_str(), "ptt" | "ptc"))
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
    core.unregister_all();
}
#[tauri::command]
fn rec_cancel(core: C) {
    core.register_all();
    core.emit_state();
}
#[tauri::command]
fn set_shortcut(core: C, action: String, combo: String) -> Result<(), String> {
    core.set_shortcut(&action, &combo)
}
#[tauri::command]
fn clear_shortcut(core: C, action: String) {
    core.clear_shortcut(&action);
}
#[tauri::command]
fn reset_shortcut(core: C) -> Result<(), String> {
    core.set_shortcut(TOGGLE_MUTE, DEFAULT_SHORTCUT)
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
fn dot_ready(core: C, window: WebviewWindow) {
    place_dot(&core.app, window.label(), &core.settings().dot_corner);
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
    let mics = audio::mics();
    let target = audio::target(&mics, s.mic.as_deref());
    let core = Arc::new(Core {
        app: app.clone(),
        muted: AtomicBool::new(audio::get_muted(&target).unwrap_or(false)),
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
        sc_errs: Mutex::new(BTreeMap::new()),
        last: Mutex::new(format!("wpctl get-volume {target}")),
        mics: Mutex::new(mics),
        target: Mutex::new(target),
        running: Mutex::new(HashSet::new()),
        tray: Mutex::new(None),
        hold: Mutex::new(ptt::Hold::default()),
        meet: Mutex::new(None),
        cmd_src: Mutex::new(String::new()),
        backend: shortcuts::detect(),
        portal: Mutex::new(None),
    });
    app.manage(core.clone());
    if core.backend == shortcuts::Backend::Portal {
        let c = core.clone();
        match shortcuts::Portal::start(move |id, pressed| c.on_key(id, if pressed { ShortcutState::Pressed } else { ShortcutState::Released })) {
            Ok(p) => *core.portal.lock().unwrap() = Some(p),
            Err(e) => eprintln!("muttei: global shortcuts portal: {e}"),
        }
    }
    if core.backend != shortcuts::Backend::Gnome && shortcuts::gnome_has_ours() {
        let _ = shortcuts::gnome_apply(&[], &Core::exe());
    }

    WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("Ajustes do Muttei")
        .inner_size(820.0, 640.0)
        .min_inner_size(560.0, 480.0)
        .decorations(false)
        .transparent(true)
        .visible(first_run)
        .build()?;
    place_pills(app);
    ensure_dots(app, &s.dot_corner);

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
    core.register_all();
    core.update_indicators();
    if let Some(a) = action_arg(std::env::args()) {
        core.on_key(&a, ShortcutState::Pressed);
    }
    core.restart_phone();
    tauri::async_runtime::spawn(remote::serve_meet(core.clone()));

    let c = core.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(500));
        // ponytail: polling wpctl; switch to pw-mon events if it ever shows up in CPU
        let changed = {
            let _g = c.audio_lock.lock().unwrap();
            match audio::get_muted(&c.target()) {
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
            c.refresh_mics();
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
    // pill and dot need to place themselves and stay on top, which native Wayland forbids; XWayland allows it
    if shortcuts::wayland() && std::env::var_os("GDK_BACKEND").is_none() {
        std::env::set_var("GDK_BACKEND", "x11");
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _| match action_arg(argv) {
            Some(a) => app.state::<Arc<Core>>().on_key(&a, ShortcutState::Pressed),
            None => show_settings(app),
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .invoke_handler(tauri::generate_handler![
            get_state, toggle_mute, update, rec_start, rec_cancel, set_shortcut, clear_shortcut, reset_shortcut, regen_code, revoke, pair_window, test_sound, lock_now, hide_settings, dot_ready
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

#[cfg(test)]
mod tests {
    use super::action_arg;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_action_argument() {
        assert_eq!(action_arg(args(&["muttei", "--action", "toggle_mute"])), Some("toggle_mute".into()));
        assert_eq!(action_arg(args(&["muttei", "--action", "camera"])), Some("camera".into()));
        assert_eq!(action_arg(args(&["muttei", "--action", "ptt"])), None);
        assert_eq!(action_arg(args(&["muttei", "--action", "rm"])), None);
        assert_eq!(action_arg(args(&["muttei"])), None);
    }
}
