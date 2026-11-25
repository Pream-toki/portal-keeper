use std::ffi::c_void;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::NetworkManagement::IpHelper::*;
use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows_sys::Win32::NetworkManagement::WiFi::*;
use windows_sys::Win32::Networking::WinSock::*;

/// Currently associated SSID, read in-process (no `netsh` subprocess, ~0 ms).
pub fn ssid() -> Option<String> {
    unsafe {
        let mut handle: HANDLE = null_mut();
        let mut negotiated = 0u32;
        if WlanOpenHandle(2, null(), &mut negotiated, &mut handle) != 0 {
            return None;
        }
        let mut list: *mut WLAN_INTERFACE_INFO_LIST = null_mut();
        if WlanEnumInterfaces(handle, null(), &mut list) != 0 {
            WlanCloseHandle(handle, null());
            return None;
        }
        let mut found = None;
        let count = (*list).dwNumberOfItems as usize;
        for i in 0..count {
            let info = &(*list).InterfaceInfo[i];
            if info.isState != wlan_interface_state_connected {
                continue;
            }
            let mut size = 0u32;
            let mut data: *mut c_void = null_mut();
            if WlanQueryInterface(
                handle,
                &info.InterfaceGuid,
                wlan_intf_opcode_current_connection,
                null(),
                &mut size,
                &mut data,
                null_mut(),
            ) == 0
            {
                let attrs = data as *const WLAN_CONNECTION_ATTRIBUTES;
                let assoc = &(*attrs).wlanAssociationAttributes;
                let len = (assoc.dot11Ssid.uSSIDLength as usize).min(32);
                let bytes = &assoc.dot11Ssid.ucSSID[..len];
                found = Some(String::from_utf8_lossy(bytes).trim().to_string());
                WlanFreeMemory(data);
                break;
            }
        }
        WlanFreeMemory(list as *mut c_void);
        WlanCloseHandle(handle, null());
        found.filter(|s| !s.is_empty())
    }
}

/// IPv4 default gateway of the adapter that actually has a route (the campus router).
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
