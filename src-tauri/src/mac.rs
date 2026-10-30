use crate::audio::{Mic, DEFAULT};
use std::collections::HashSet;
use std::ffi::{c_char, c_void, CStr, CString};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

#[repr(C)]
struct Addr {
    selector: u32,
    scope: u32,
    element: u32,
}

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectHasProperty(id: u32, addr: *const Addr) -> u8;
    fn AudioObjectIsPropertySettable(id: u32, addr: *const Addr, settable: *mut u8) -> i32;
    fn AudioObjectGetPropertyDataSize(id: u32, addr: *const Addr, qsize: u32, qdata: *const c_void, size: *mut u32) -> i32;
    fn AudioObjectGetPropertyData(id: u32, addr: *const Addr, qsize: u32, qdata: *const c_void, size: *mut u32, data: *mut c_void) -> i32;
    fn AudioObjectSetPropertyData(id: u32, addr: *const Addr, qsize: u32, qdata: *const c_void, size: u32, data: *const c_void) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringGetCString(s: *const c_void, buf: *mut c_char, size: isize, encoding: u32) -> u8;
    fn CFStringCreateWithCString(alloc: *const c_void, s: *const c_char, encoding: u32) -> *const c_void;
    fn CFDictionaryGetValue(dict: *const c_void, key: *const c_void) -> *const c_void;
    fn CFRelease(obj: *const c_void);
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGSessionCopyCurrentDictionary() -> *const c_void;
}

extern "C" {
    fn dlopen(path: *const c_char, mode: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

const fn fourcc(s: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*s)
}

const SYSTEM: u32 = 1;
const GLOBAL: u32 = fourcc(b"glob");
const INPUT: u32 = fourcc(b"inpt");
const DEVICES: u32 = fourcc(b"dev#");
const DEFAULT_INPUT: u32 = fourcc(b"dIn ");
const STREAMS: u32 = fourcc(b"stm#");
const UID: u32 = fourcc(b"uid ");
const NAME: u32 = fourcc(b"lnam");
const MUTE: u32 = fourcc(b"mute");
const VOLUME: u32 = fourcc(b"volm");
const UTF8: u32 = 0x0800_0100;

fn addr(selector: u32, scope: u32, element: u32) -> Addr {
    Addr { selector, scope, element }
}

fn get<T: Copy + Default>(id: u32, a: &Addr) -> Option<T> {
    let mut v = T::default();
    let mut size = std::mem::size_of::<T>() as u32;
    let st = unsafe { AudioObjectGetPropertyData(id, a, 0, std::ptr::null(), &mut size, &mut v as *mut T as *mut c_void) };
    (st == 0).then_some(v)
}

fn set<T>(id: u32, a: &Addr, v: T) -> bool {
    unsafe { AudioObjectSetPropertyData(id, a, 0, std::ptr::null(), std::mem::size_of::<T>() as u32, &v as *const T as *const c_void) == 0 }
}

fn settable(id: u32, a: &Addr) -> bool {
    let mut s = 0u8;
    unsafe { AudioObjectHasProperty(id, a) != 0 && AudioObjectIsPropertySettable(id, a, &mut s) == 0 && s != 0 }
}

fn cf_string(s: *const c_void) -> Option<String> {
    if s.is_null() {
        return None;
    }
    let mut buf = [0 as c_char; 512];
    let ok = unsafe { CFStringGetCString(s, buf.as_mut_ptr(), buf.len() as isize, UTF8) } != 0;
    unsafe { CFRelease(s) };
    ok.then(|| unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned())
}

fn string_prop(id: u32, selector: u32) -> Option<String> {
    cf_string(get::<usize>(id, &addr(selector, GLOBAL, 0))? as *const c_void)
}

fn devices() -> Vec<u32> {
    let a = addr(DEVICES, GLOBAL, 0);
    let mut size = 0u32;
    if unsafe { AudioObjectGetPropertyDataSize(SYSTEM, &a, 0, std::ptr::null(), &mut size) } != 0 {
        return vec![];
    }
    let mut ids = vec![0u32; size as usize / 4];
    let st = unsafe { AudioObjectGetPropertyData(SYSTEM, &a, 0, std::ptr::null(), &mut size, ids.as_mut_ptr() as *mut c_void) };
    if st != 0 {
        return vec![];
    }
    ids.truncate(size as usize / 4);
    ids
}

fn has_input(id: u32) -> bool {
    let mut size = 0u32;
    unsafe { AudioObjectGetPropertyDataSize(id, &addr(STREAMS, INPUT, 0), 0, std::ptr::null(), &mut size) == 0 && size > 0 }
}

fn device(target: &str) -> Option<u32> {
    if target == DEFAULT {
        return get::<u32>(SYSTEM, &addr(DEFAULT_INPUT, GLOBAL, 0)).filter(|&d| d != 0);
    }
    devices().into_iter().find(|&d| string_prop(d, UID).as_deref() == Some(target))
}

// many Mac mics (the built-in one included) have no mute control; those get their input volume zeroed instead
fn volume_elements(id: u32) -> Vec<u32> {
    [0, 1, 2].into_iter().filter(|&e| settable(id, &addr(VOLUME, INPUT, e))).collect()
}

static SAVED: Mutex<Vec<(u32, u32, f32)>> = Mutex::new(Vec::new());

pub fn get_muted(target: &str) -> Option<bool> {
    let id = device(target)?;
    let mute = addr(MUTE, INPUT, 0);
    if unsafe { AudioObjectHasProperty(id, &mute) } != 0 {
        return get::<u32>(id, &mute).map(|m| m != 0);
    }
    let e = *volume_elements(id).first()?;
    get::<f32>(id, &addr(VOLUME, INPUT, e)).map(|v| v == 0.0)
}

pub fn set_muted(target: &str, muted: bool) -> bool {
    let Some(id) = device(target) else { return false };
    let mute = addr(MUTE, INPUT, 0);
    if settable(id, &mute) {
        return set(id, &mute, muted as u32);
    }
    let elems = volume_elements(id);
    if elems.is_empty() {
        return false;
    }
    let mut saved = SAVED.lock().unwrap();
    elems.iter().all(|&e| {
        let a = addr(VOLUME, INPUT, e);
        let v = if muted {
            let cur = get::<f32>(id, &a).unwrap_or(1.0);
            saved.retain(|s| (s.0, s.1) != (id, e));
            if cur > 0.0 {
                saved.push((id, e, cur));
            }
            0.0
        } else {
            // ponytail: muted before Muttei started, so the old level is unknown; 0.75 is a sane guess
            saved.iter().find(|s| (s.0, s.1) == (id, e)).map(|s| s.2).unwrap_or(0.75)
        };
        set(id, &a, v)
    })
}

pub fn set_default(uid: &str) -> bool {
    device(uid).is_some_and(|id| set(SYSTEM, &addr(DEFAULT_INPUT, GLOBAL, 0), id))
}

pub fn mics() -> Vec<Mic> {
    let def = device(DEFAULT);
    devices()
        .into_iter()
        .filter(|&d| has_input(d))
        .filter_map(|d| {
            let uid = string_prop(d, UID)?;
            Some(Mic { id: uid.clone(), desc: string_prop(d, NAME).unwrap_or_else(|| uid.clone()), def: def == Some(d), name: uid, ..Default::default() })
        })
        .collect()
}

pub fn play(muted: bool) {
    let file = if muted { "Tink" } else { "Pop" };
    let _ = Command::new("afplay").arg(format!("/System/Library/Sounds/{file}.aiff")).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

pub fn running() -> HashSet<String> {
    Command::new("ps")
        .args(["-axco", "comm="])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

fn locked() -> bool {
    let dict = unsafe { CGSessionCopyCurrentDictionary() };
    if dict.is_null() {
        return false;
    }
    let key = CString::new("CGSSessionScreenIsLocked").unwrap();
    let k = unsafe { CFStringCreateWithCString(std::ptr::null(), key.as_ptr(), UTF8) };
    let locked = !unsafe { CFDictionaryGetValue(dict, k) }.is_null();
    unsafe {
        CFRelease(k);
        CFRelease(dict);
    }
    locked
}

// ponytail: 1s poll; the distributed notification "com.apple.screenIsLocked" needs an NSRunLoop observer
pub fn watch_lock(on_change: impl Fn(bool) + Send + 'static) {
    std::thread::spawn(move || {
        let mut prev = false;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let now = locked();
            if now != prev {
                prev = now;
                on_change(now);
            }
        }
    });
}

// the login framework is private, but it is what the Ctrl+Cmd+Q menu item calls
pub fn lock_now() {
    let path = CString::new("/System/Library/PrivateFrameworks/login.framework/Versions/Current/login").unwrap();
    let sym = CString::new("SACLockScreenImmediate").unwrap();
    unsafe {
        let h = dlopen(path.as_ptr(), 1);
        if h.is_null() {
            return;
        }
        let f = dlsym(h, sym.as_ptr());
        if !f.is_null() {
            let f: extern "C" fn() -> i32 = std::mem::transmute(f);
            f();
        }
    }
}
