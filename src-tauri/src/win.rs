use crate::audio::{Mic, DEFAULT};
use std::collections::HashSet;
use std::time::Duration;
use windows::core::{interface, IUnknown, IUnknown_Vtbl, Result, GUID, HRESULT, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eCapture, eCommunications, eConsole, eMultimedia, ERole, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, PlaySoundW, DEVICE_STATE_ACTIVE, SND_ALIAS, SND_ASYNC};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ};
use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::System::StationsAndDesktops::{CloseDesktop, OpenInputDesktop, SwitchDesktop, DESKTOP_CONTROL_FLAGS, DESKTOP_SWITCHDESKTOP};

fn enumerator() -> Result<IMMDeviceEnumerator> {
    // already-initialized threads return S_FALSE, which is fine
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
}

fn device(target: &str) -> Result<IMMDevice> {
    let en = enumerator()?;
    unsafe {
        if target == DEFAULT {
            en.GetDefaultAudioEndpoint(eCapture, eConsole)
        } else {
            en.GetDevice(&HSTRING::from(target))
        }
    }
}

fn volume(target: &str) -> Result<IAudioEndpointVolume> {
    unsafe { device(target)?.Activate(CLSCTX_ALL, None) }
}

pub fn get_muted(target: &str) -> Option<bool> {
    volume(target).and_then(|v| unsafe { v.GetMute() }).ok().map(|b| b.as_bool())
}

pub fn set_muted(target: &str, muted: bool) -> bool {
    volume(target).and_then(|v| unsafe { v.SetMute(muted, std::ptr::null()) }).is_ok()
}

fn take(p: PWSTR) -> String {
    let s = unsafe { p.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(p.0 as _)) };
    s
}

fn id(d: &IMMDevice) -> Option<String> {
    unsafe { d.GetId() }.ok().map(take)
}

fn friendly(d: &IMMDevice) -> Option<String> {
    let store = unsafe { d.OpenPropertyStore(STGM_READ) }.ok()?;
    let v = unsafe { store.GetValue(&PKEY_Device_FriendlyName) }.ok()?;
    Some(v.to_string())
}

// IPolicyConfig is undocumented but stable since Vista; the Sound control panel uses it to set the default device.
// Only the vtable slot of SetDefaultEndpoint matters, so the methods before it are placeholders.
#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
    fn get_mix_format(&self) -> HRESULT;
    fn get_device_format(&self) -> HRESULT;
    fn reset_device_format(&self) -> HRESULT;
    fn set_device_format(&self) -> HRESULT;
    fn get_processing_period(&self) -> HRESULT;
    fn set_processing_period(&self) -> HRESULT;
    fn get_share_mode(&self) -> HRESULT;
    fn set_share_mode(&self) -> HRESULT;
    fn get_property_value(&self) -> HRESULT;
    fn set_property_value(&self) -> HRESULT;
    fn set_default_endpoint(&self, id: PCWSTR, role: ERole) -> HRESULT;
}

const POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

pub fn set_default(id: &str) -> bool {
    let _ = enumerator();
    let Ok(pc) = (unsafe { CoCreateInstance::<_, IPolicyConfig>(&POLICY_CONFIG_CLIENT, None, CLSCTX_ALL) }) else { return false };
    let id = HSTRING::from(id);
    [eConsole, eMultimedia, eCommunications].into_iter().all(|role| unsafe { pc.set_default_endpoint(PCWSTR(id.as_ptr()), role) }.is_ok())
}

pub fn mics() -> Vec<Mic> {
    let Ok(en) = enumerator() else { return vec![] };
    let def = unsafe { en.GetDefaultAudioEndpoint(eCapture, eConsole) }.ok().and_then(|d| id(&d));
    let Ok(list) = (unsafe { en.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) }) else { return vec![] };
    let n = unsafe { list.GetCount() }.unwrap_or(0);
    (0..n)
        .filter_map(|i| {
            let d = unsafe { list.Item(i) }.ok()?;
            let name = id(&d)?;
            Some(Mic { id: name.clone(), desc: friendly(&d).unwrap_or_else(|| name.clone()), def: def.as_deref() == Some(name.as_str()), name, ..Default::default() })
        })
        .collect()
}

pub fn play(muted: bool) {
    let alias = HSTRING::from(if muted { "DeviceDisconnect" } else { "DeviceConnect" });
    let _ = unsafe { PlaySoundW(&alias, None, SND_ALIAS | SND_ASYNC) };
}

pub fn running() -> HashSet<String> {
    let mut names = HashSet::new();
    let Ok(snap) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else { return names };
    let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut ok = unsafe { Process32FirstW(snap, &mut e) }.is_ok();
    while ok {
        let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
        let exe = String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase();
        names.insert(exe.strip_suffix(".exe").unwrap_or(&exe).to_string());
        ok = unsafe { Process32NextW(snap, &mut e) }.is_ok();
    }
    let _ = unsafe { CloseHandle(snap) };
    names
}

// the input desktop can't be switched to while the lock screen owns it
fn locked() -> bool {
    match unsafe { OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_SWITCHDESKTOP) } {
        Ok(h) => {
            let locked = !unsafe { SwitchDesktop(h) }.is_ok();
            let _ = unsafe { CloseDesktop(h) };
            locked
        }
        Err(_) => true,
    }
}

// ponytail: 1s poll; WTSRegisterSessionNotification needs a window proc, worth it only if the delay bothers
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

pub fn lock_now() {
    let _ = unsafe { LockWorkStation() };
}
