use crate::{Core, Event, MeetStatus, MEET_CMDS};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use rand::Rng;
use serde::Serialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

pub const MEET_PORT: u16 = 47822;
const LOCKOUT: Duration = Duration::from_secs(60);
const HIDDEN: &[&str] = &["docker", "br-", "veth", "tun", "wg", "virbr", "vEthernet", "Loopback", "VirtualBox", "VMware", "utun", "awdl", "llw", "anpi", "bridge", "gif", "stf", "ap1"];

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Iface {
    pub name: String,
    pub ip: Option<String>,
}

pub fn is_hidden(name: &str) -> bool {
    name == "lo" || HIDDEN.iter().any(|p| name.starts_with(p))
}

pub fn hidden_summary(names: &[String]) -> String {
    let mut groups: Vec<(String, Vec<&String>)> = vec![];
    for n in names.iter().filter(|n| is_hidden(n)) {
        let key = if n == "lo" { "lo".into() } else { HIDDEN.iter().find(|p| n.starts_with(*p)).unwrap().to_string() };
        match groups.iter_mut().find(|g| g.0 == key) {
            Some(g) => g.1.push(n),
            None => groups.push((key, vec![n])),
        }
    }
    groups
        .iter()
        .map(|(k, v)| if v.len() == 1 { v[0].clone() } else { format!("{} (+{} {k}*)", v[0], v.len() - 1) })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn interfaces() -> (Vec<Iface>, String) {
    let addrs = if_addrs::get_if_addrs().unwrap_or_default();
    // /sys also lists interfaces that are down; Windows only has what if_addrs reports
    let mut names: Vec<String> = std::fs::read_dir("/sys/class/net")
        .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_else(|_| addrs.iter().filter(|a| !a.is_loopback()).map(|a| a.name.clone()).collect());
    names.sort();
    names.dedup();
    let list = names
        .iter()
        .filter(|n| !is_hidden(n))
        .map(|n| Iface {
            name: n.clone(),
            ip: addrs.iter().find(|a| &a.name == n && a.ip().is_ipv4()).map(|a| a.ip().to_string()),
        })
        .collect();
    (list, hidden_summary(&names))
}

pub fn pick(list: &[Iface], pref: &Option<String>) -> Option<Iface> {
    let up = |i: &&Iface| i.ip.is_some();
    pref.as_ref()
        .and_then(|p| list.iter().filter(up).find(|i| &i.name == p))
        .or_else(|| list.iter().find(up))
        .cloned()
}

pub fn ufw_enabled() -> bool {
    std::fs::read_to_string("/etc/ufw/ufw.conf").map(|s| s.lines().any(|l| l.trim() == "ENABLED=yes")).unwrap_or(false)
}

pub fn qr_svg(url: &str) -> String {
    use qrcode::render::svg;
    qrcode::QrCode::new(url)
        .map(|c| c.render::<svg::Color>().min_dimensions(116, 116).quiet_zone(false).dark_color(svg::Color("#191a21")).light_color(svg::Color("#f8f8f2")).build())
        .unwrap_or_default()
}

pub fn token() -> String {
    let mut r = rand::rng();
    (0..32).map(|_| format!("{:x}", r.random_range(0..16u8))).collect()
}

#[derive(Debug, PartialEq)]
pub enum PairResult {
    Ok,
    Bad { left: u8 },
    Regenerated,
    Locked { secs: u64 },
}

pub struct Pairing {
    pub code: String,
    pub fails: u8,
    locked_until: Option<Instant>,
}

impl Pairing {
    pub fn new() -> Self {
        Self { code: Self::new_code(), fails: 0, locked_until: None }
    }

    pub fn locked_secs(&self) -> u64 {
        self.locked_until.map(|t| t.saturating_duration_since(Instant::now()).as_secs()).filter(|s| *s > 0).unwrap_or(0)
    }

    fn new_code() -> String {
        rand::rng().random_range(1000..10000).to_string()
    }

    pub fn regen(&mut self) {
        self.code = Self::new_code();
        self.fails = 0;
    }

    pub fn try_code(&mut self, code: &str) -> PairResult {
        let secs = self.locked_secs();
        if secs > 0 {
            return PairResult::Locked { secs };
        }
        if code == self.code {
            self.regen();
            return PairResult::Ok;
        }
        self.fails += 1;
        if self.fails >= 5 {
            self.regen();
            self.locked_until = Some(Instant::now() + LOCKOUT);
            return PairResult::Regenerated;
        }
        PairResult::Bad { left: 5 - self.fails }
    }
}

fn phone_state(core: &Core, host: &str) -> Value {
    json!({"t":"state","muted":core.muted(),"host":host,"meet":core.meet_status()})
}

fn device_name(ua: &str) -> String {
    let ua = ua.to_lowercase();
    if ua.contains("iphone") { "iPhone" } else if ua.contains("ipad") { "iPad" } else if ua.contains("android") { "Android" } else { "Navegador" }.into()
}

async fn send(ws: &mut WebSocket, v: Value) -> bool {
    ws.send(Message::Text(v.to_string().into())).await.is_ok()
}

fn woff2(b: &'static [u8]) -> impl IntoResponse {
    ([(axum::http::header::CONTENT_TYPE, "font/woff2"), (axum::http::header::CACHE_CONTROL, "max-age=604800")], b)
}

pub async fn serve_phone(core: Arc<Core>, addr: SocketAddr, stop: oneshot::Receiver<()>) -> std::io::Result<()> {
    let app = Router::new()
        .route("/", get(|| async { Html(include_str!("../../ui/phone.html")) }))
        .route("/logo.svg", get(|| async { ([(axum::http::header::CONTENT_TYPE, "image/svg+xml")], include_str!("../../ui/logo-lockup.svg")) }))
        .route("/fonts/IBMPlexSans.woff2", get(|| async { woff2(include_bytes!("../../ui/fonts/IBMPlexSans.woff2")) }))
        .route("/fonts/JetBrainsMono.woff2", get(|| async { woff2(include_bytes!("../../ui/fonts/JetBrainsMono.woff2")) }))
        .route("/ws", get(phone_ws))
        .with_state(core);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).with_graceful_shutdown(async { let _ = stop.await; }).await
}

async fn phone_ws(ws: WebSocketUpgrade, headers: HeaderMap, State(core): State<Arc<Core>>) -> impl IntoResponse {
    let ua = headers.get("user-agent").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    ws.on_upgrade(move |s| phone_loop(s, core, ua))
}

async fn phone_loop(mut ws: WebSocket, core: Arc<Core>, ua: String) {
    let mut rx = core.tx.subscribe();
    let mut authed: Option<String> = None;
    let host = core.hostname();
    loop {
        tokio::select! {
            m = ws.recv() => {
                let Some(Ok(Message::Text(t))) = m else {
                    if matches!(m, Some(Ok(_))) { continue } else { break }
                };
                let v: Value = serde_json::from_str(&t).unwrap_or_default();
                let tok = v["token"].as_str().unwrap_or("").to_string();
                match v["t"].as_str().unwrap_or("") {
                    "pair" => {
                        let reply = match core.pair(v["code"].as_str().unwrap_or(""), &device_name(&ua)) {
                            Ok(tok) => { authed = Some(tok.clone()); json!({"t":"paired","token":tok,"muted":core.muted(),"host":host,"meet":core.meet_status()}) }
                            Err(PairResult::Regenerated) => json!({"t":"regen"}),
                            Err(PairResult::Bad { left }) => json!({"t":"bad","left":left}),
                            Err(PairResult::Locked { secs }) => json!({"t":"locked","secs":secs}),
                            Err(PairResult::Ok) => unreachable!(),
                        };
                        if !send(&mut ws, reply).await { break }
                    }
                    "hello" | "toggle" | "cmd" if core.valid_token(&tok) => {
                        authed = Some(tok);
                        let reply = match v["t"].as_str() {
                            Some("toggle") => json!({"t":"ack","muted":core.toggle("celular")}),
                            Some("cmd") => {
                                let cmd = v["cmd"].as_str().filter(|c| MEET_CMDS.contains(c)).unwrap_or("");
                                json!({"t":"ack","muted":core.muted(),"cmd":cmd,"ok":!cmd.is_empty() && core.meet_cmd(cmd, "celular")})
                            }
                            _ => phone_state(&core, &host),
                        };
                        if !send(&mut ws, reply).await { break }
                    }
                    "hello" | "toggle" | "cmd" => {
                        authed = None;
                        if !send(&mut ws, json!({"t":"revoked"})).await { break }
                    }
                    _ => {}
                }
            }
            e = rx.recv() => {
                let Ok(e) = e else { continue };
                let Some(tok) = authed.clone() else { continue };
                let msg = match e {
                    Event::Muted(_) | Event::Meet => phone_state(&core, &host),
                    Event::Devices if !core.valid_token(&tok) => { authed = None; json!({"t":"revoked"}) }
                    Event::Devices | Event::Cmd(_) => continue,
                };
                if !send(&mut ws, msg).await { break }
            }
        }
    }
}

pub async fn serve_meet(core: Arc<Core>) {
    let app = Router::new().route("/", get(meet_ws)).with_state(core);
    match tokio::net::TcpListener::bind(("127.0.0.1", MEET_PORT)).await {
        Ok(l) => { let _ = axum::serve(l, app).await; }
        Err(e) => eprintln!("muttei: meet server on 127.0.0.1:{MEET_PORT} failed: {e}"),
    }
}

pub fn extension_id(origin: &str) -> Option<String> {
    let id = origin.strip_prefix("chrome-extension://")?.trim_end_matches('/');
    (!id.is_empty() && id.bytes().all(|b| b.is_ascii_lowercase())).then(|| id.to_string())
}

async fn meet_ws(ws: WebSocketUpgrade, headers: HeaderMap, State(core): State<Arc<Core>>) -> axum::response::Response {
    let origin = headers.get("origin").and_then(|v| v.to_str().ok()).unwrap_or("");
    let Some(id) = extension_id(origin) else {
        return (axum::http::StatusCode::FORBIDDEN, "origin not allowed").into_response();
    };
    ws.on_upgrade(move |s| meet_loop(s, core, id)).into_response()
}

async fn meet_loop(mut ws: WebSocket, core: Arc<Core>, id: String) {
    let mut rx = core.tx.subscribe();
    let mut live = false;
    loop {
        tokio::select! {
            m = ws.recv() => {
                let Some(Ok(Message::Text(t))) = m else {
                    if matches!(m, Some(Ok(_))) { continue } else { break }
                };
                let v: Value = serde_json::from_str(&t).unwrap_or_default();
                match v["t"].as_str().unwrap_or("") {
                    "hello" if !live => {
                        if !core.meet_approve(&id, v["browser"].as_str().unwrap_or("Navegador")) {
                            let _ = send(&mut ws, json!({"t":"denied"})).await;
                            break;
                        }
                        live = true;
                        core.meet_live(&id, true);
                        if !send(&mut ws, json!({"t":"state","muted":core.muted()})).await { break }
                    }
                    "ping" if !send(&mut ws, json!({"t":"pong"})).await => break,
                    "status" if live => core.set_meet_status(Some(MeetStatus {
                        in_call: v["in_call"].as_bool().unwrap_or(false),
                        cam: v["cam"].as_bool(),
                        hand: v["hand"].as_bool().unwrap_or(false),
                    })),
                    "result" if live => core.meet_result(v["cmd"].as_str().unwrap_or(""), v["ok"].as_bool().unwrap_or(false), v["on"].as_bool()),
                    _ => {}
                }
            }
            e = rx.recv() => {
                let msg = match (live, e) {
                    (true, Ok(Event::Muted(m))) => json!({"t":"state","muted":m}),
                    (true, Ok(Event::Cmd(c))) => json!({"t":"cmd","cmd":c}),
                    _ => continue,
                };
                if !send(&mut ws, msg).await { break }
            }
        }
    }
    if live {
        core.meet_live(&id, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_virtual_interfaces() {
        for n in ["lo", "docker0", "br-123b541097b7", "veth1ad743c", "tun0", "wg0", "virbr0"] {
            assert!(is_hidden(n), "{n}");
        }
        for n in ["wlp0s20f3", "enp3s0", "eth0"] {
            assert!(!is_hidden(n), "{n}");
        }
    }

    #[test]
    fn summarizes_hidden() {
        let names: Vec<String> = ["lo", "docker0", "br-a", "br-b", "br-c", "wlp0s20f3"].iter().map(|s| s.to_string()).collect();
        assert_eq!(hidden_summary(&names), "lo, docker0, br-a (+2 br-*)");
    }

    #[test]
    fn picks_preferred_or_first_up() {
        let l = vec![Iface { name: "enp3s0".into(), ip: None }, Iface { name: "wlp0s20f3".into(), ip: Some("192.168.0.157".into()) }];
        assert_eq!(pick(&l, &None).unwrap().name, "wlp0s20f3");
        assert_eq!(pick(&l, &Some("enp3s0".into())).unwrap().name, "wlp0s20f3");
    }

    #[test]
    fn pairing_flow() {
        let mut p = Pairing::new();
        let code = p.code.clone();
        let wrong = if code == "1000" { "1001" } else { "1000" };
        assert_eq!(p.try_code(wrong), PairResult::Bad { left: 4 });
        assert_eq!(p.try_code(&code), PairResult::Ok);
        assert_eq!(p.fails, 0);

        let code = p.code.clone();
        let wrong = if code == "1000" { "1001" } else { "1000" };
        for left in (1..5).rev() {
            assert_eq!(p.try_code(wrong), PairResult::Bad { left });
        }
        assert_eq!(p.try_code(wrong), PairResult::Regenerated);
        assert_eq!(p.fails, 0);
        let fresh = p.code.clone();
        assert!(matches!(p.try_code(&fresh), PairResult::Locked { secs } if secs > 50));
        assert!(matches!(p.try_code(wrong), PairResult::Locked { .. }));
        assert_eq!(p.code, fresh);
    }

    #[test]
    fn origin_check() {
        assert_eq!(extension_id("chrome-extension://abcdefghijklmnopabcdefghijklmnop"), Some("abcdefghijklmnopabcdefghijklmnop".into()));
        assert_eq!(extension_id("https://meet.google.com"), None);
        assert_eq!(extension_id("chrome-extension://"), None);
        assert_eq!(extension_id(""), None);
    }
}
