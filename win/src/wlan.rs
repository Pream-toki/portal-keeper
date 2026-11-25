use std::ffi::c_void;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::NetworkManagement::IpHelper::*;
use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows_sys::Win32::NetworkManagement::WiFi::*;
use windows_sys::Win32::Networking::WinSock::*;

pub struct Wifi {
    pub ssid: String,
    pub signal: u32,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn open() -> Option<HANDLE> {
    let mut h: HANDLE = null_mut();
    let mut negotiated = 0u32;
    if WlanOpenHandle(2, null(), &mut negotiated, &mut h) != 0 {
        return None;
    }
    Some(h)
}

unsafe fn first_interface(h: HANDLE) -> Option<windows_sys::core::GUID> {
    let mut list: *mut WLAN_INTERFACE_INFO_LIST = null_mut();
    if WlanEnumInterfaces(h, null(), &mut list) != 0 {
        return None;
    }
    let mut guid = None;
    if (*list).dwNumberOfItems > 0 {
        guid = Some((*list).InterfaceInfo[0].InterfaceGuid);
    }
    WlanFreeMemory(list as *mut c_void);
    guid
}

/// Connected SSID and its signal quality, read in-process (no subprocess).
pub fn current() -> Option<Wifi> {
    unsafe {
        let h = open()?;
        let guid = first_interface(h)?;
        let mut size = 0u32;
        let mut data: *mut c_void = null_mut();
        let mut found = None;
        if WlanQueryInterface(
            h,
            &guid,
            wlan_intf_opcode_current_connection,
            null(),
            &mut size,
            &mut data,
            null_mut(),
        ) == 0
        {
            let attrs = data as *const WLAN_CONNECTION_ATTRIBUTES;
            if (*attrs).isState == wlan_interface_state_connected {
                let a = &(*attrs).wlanAssociationAttributes;
                let len = (a.dot11Ssid.uSSIDLength as usize).min(32);
                let ssid = String::from_utf8_lossy(&a.dot11Ssid.ucSSID[..len])
                    .trim()
                    .to_string();
                if !ssid.is_empty() {
                    found = Some(Wifi {
                        ssid,
                        signal: a.wlanSignalQuality as u32,
                    });
                }
            }
            WlanFreeMemory(data);
        }
        WlanCloseHandle(h, null());
        found
    }
}

pub fn ssid() -> Option<String> {
    current().map(|w| w.ssid)
}

/// Visible connectable networks, strongest first, one entry per name.
pub fn available() -> Vec<Wifi> {
    unsafe {
        let h = match open() {
            Some(h) => h,
            None => return Vec::new(),
        };
        let guid = match first_interface(h) {
            Some(g) => g,
            None => {
                WlanCloseHandle(h, null());
                return Vec::new();
            }
        };
        let mut list: *mut WLAN_AVAILABLE_NETWORK_LIST = null_mut();
        if WlanGetAvailableNetworkList(h, &guid, 0, null(), &mut list) != 0 {
            WlanCloseHandle(h, null());
            return Vec::new();
        }
        let mut out = Vec::new();
        let base = (*list).Network.as_ptr();
        for i in 0..(*list).dwNumberOfItems as usize {
            let n = &*base.add(i);
            if n.bNetworkConnectable == 0 {
                continue;
            }
            let len = (n.dot11Ssid.uSSIDLength as usize).min(32);
            let ssid = String::from_utf8_lossy(&n.dot11Ssid.ucSSID[..len])
                .trim()
                .to_string();
            if ssid.is_empty() {
                continue;
            }
            out.push(Wifi {
                ssid,
                signal: n.wlanSignalQuality as u32,
            });
        }
        WlanFreeMemory(list as *mut c_void);
        WlanCloseHandle(h, null());
        out.sort_by(|a, b| b.signal.cmp(&a.signal));
        out.dedup_by(|a, b| a.ssid == b.ssid);
        out
    }
}

unsafe fn ensure_profile(h: HANDLE, guid: &windows_sys::core::GUID, ssid: &str) -> bool {
    let name = wide(ssid);
    let mut xml: *mut u16 = null_mut();
    let mut flags = 0u32;
    let mut access = 0u32;
    if WlanGetProfile(h, guid, name.as_ptr(), null(), &mut xml, &mut flags, &mut access) == 0
        && !xml.is_null()
    {
        WlanFreeMemory(xml as *mut c_void);
        return true;
    }
    let esc = ssid
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let profile = format!(
        r#"<?xml version="1.0"?><WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1"><name>{esc}</name><SSIDConfig><SSID><name>{esc}</name></SSID></SSIDConfig><connectionType>ESS</connectionType><connectionMode>auto</connectionMode><MSM><security><authEncryption><authentication>open</authentication><encryption>none</encryption><useOneX>false</useOneX></authEncryption></security></MSM></WLANProfile>"#
    );
    let xml_wide = wide(&profile);
    let mut reason = 0u32;
    WlanSetProfile(
        h,
        guid,
        0,
        xml_wide.as_ptr(),
        null(),
        0,
        null(),
        &mut reason,
    ) == 0
}

/// Join a network by name.
///
/// The profile is created through the WLAN API when Windows has none, but the join itself goes
/// through `netsh` so the exact SSID is honoured instead of letting Windows roam to a stronger
/// saved network. Only runs on an explicit switch, never in the polling loop.
fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Map a configured name onto the spelling the router actually broadcasts.
pub fn resolve(name: &str) -> String {
    let want = norm(name);
    let scan = available();
    let hit = scan.iter().find(|w| norm(&w.ssid) == want);
    if let Some(w) = hit {
        return w.ssid.clone();
    }
    if let Some(cur) = current() {
        if norm(&cur.ssid) == want {
            return cur.ssid;
        }
    }
    name.to_string()
}

pub fn connect(name: &str) -> bool {
    let ssid = resolve(name);
    let ssid = ssid.as_str();
    if let Some(cur) = current() {
        if cur.ssid.eq_ignore_ascii_case(ssid) {
            return true;
        }
    }
    unsafe {
        if let Some(h) = open() {
            if let Some(guid) = first_interface(h) {
                ensure_profile(h, &guid, ssid);
            }
            WlanCloseHandle(h, null());
        }
    }
    let mut cmd = std::process::Command::new("netsh");
    cmd.args([
        "wlan",
        "connect",
        &format!("name={ssid}"),
        &format!("ssid={ssid}"),
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    match cmd.output() {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout).to_ascii_lowercase();
            text.contains("success") || out.status.success()
        }
        Err(_) => false,
    }
}

/// Wait until the adapter actually reports the requested SSID.
pub fn wait_for(ssid: &str, seconds: u64) -> bool {
    for _ in 0..seconds * 4 {
        if let Some(cur) = current() {
            if cur.ssid.eq_ignore_ascii_case(ssid) {
                return true;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    false
}

/// IPv4 default gateway of the adapter that actually has a route.
pub fn gateway() -> Option<String> {
    unsafe {
        let mut size = 16_000u32;
        let mut buf: Vec<u8> = Vec::new();
        for _ in 0..3 {
            buf.resize(size as usize, 0);
            let rc = GetAdaptersAddresses(
                AF_INET as u32,
                GAA_FLAG_INCLUDE_GATEWAYS,
                null(),
                buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH,
                &mut size,
            );
            if rc == 0 {
                break;
            }
            if rc != 111 {
                return None;
            }
        }
        let mut adapter = buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;
        while !adapter.is_null() {
            if (*adapter).OperStatus == IfOperStatusUp {
                let mut gw = (*adapter).FirstGatewayAddress;
                while !gw.is_null() {
                    let sa = (*gw).Address.lpSockaddr;
                    if !sa.is_null() && (*sa).sa_family == AF_INET {
                        let sin = sa as *const SOCKADDR_IN;
                        let octets = (*sin).sin_addr.S_un.S_addr.to_ne_bytes();
                        if octets != [0, 0, 0, 0] {
                            return Some(format!(
                                "{}.{}.{}.{}",
                                octets[0], octets[1], octets[2], octets[3]
                            ));
                        }
                    }
                    gw = (*gw).Next;
                }
            }
            adapter = (*adapter).Next;
        }
        None
    }
}
