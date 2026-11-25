use std::ffi::c_void;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Networking::WinHttp::*;

pub struct Resp {
    pub status: u16,
    pub location: String,
    pub body: String,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn split_url(url: &str) -> Option<(bool, String, u16, String)> {
    let (secure, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return None;
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => {
            (h.to_string(), p.parse::<u16>().ok()?)
        }
        _ => (authority.to_string(), if secure { 443 } else { 80 }),
    };
    if host.is_empty() {
        return None;
    }
    Some((secure, host, port, path.to_string()))
}

pub fn get(url: &str, timeout_ms: u32) -> Option<Resp> {
    request("GET", url, None, timeout_ms)
}

pub fn post(url: &str, body: &str, timeout_ms: u32) -> Option<Resp> {
    request("POST", url, Some(body), timeout_ms)
}

fn request(method: &str, url: &str, body: Option<&str>, timeout_ms: u32) -> Option<Resp> {
    let (secure, host, port, path) = split_url(url)?;
    unsafe {
        let agent = wide("PortalKeeper/2.0");
        let session = WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_NO_PROXY,
            null(),
            null(),
            0,
        );
        if session.is_null() {
            return None;
        }
        let t = timeout_ms.min(i32::MAX as u32) as i32;
        WinHttpSetTimeouts(session, t, t, t, t);

        let host_w = wide(&host);
        let connect = WinHttpConnect(session, host_w.as_ptr(), port, 0);
        if connect.is_null() {
            WinHttpCloseHandle(session);
            return None;
        }
        let verb = wide(method);
        let object = wide(&path);
        let flags = if secure { WINHTTP_FLAG_SECURE } else { 0 };
        let req = WinHttpOpenRequest(
            connect,
            verb.as_ptr(),
            object.as_ptr(),
            null(),
            null(),
            null(),
            flags,
        );
        if req.is_null() {
            WinHttpCloseHandle(connect);
            WinHttpCloseHandle(session);
            return None;
        }

        let disable: u32 = WINHTTP_DISABLE_REDIRECTS;
        WinHttpSetOption(
            req,
            WINHTTP_OPTION_DISABLE_FEATURE,
            &disable as *const u32 as *const c_void,
            4,
        );

        let headers: Vec<u16> = if body.is_some() {
            wide("Content-Type: application/x-www-form-urlencoded\r\n")
        } else {
            wide("")
        };
        let (payload, payload_len) = match body {
            Some(b) => (b.as_ptr() as *const c_void, b.len() as u32),
            None => (null(), 0),
        };
        let sent = WinHttpSendRequest(
            req,
            headers.as_ptr(),
            u32::MAX,
            payload,
            payload_len,
            payload_len,
            0,
        );
        if sent == 0 {
            WinHttpCloseHandle(req);
            WinHttpCloseHandle(connect);
            WinHttpCloseHandle(session);
            return None;
        }
        if WinHttpReceiveResponse(req, null_mut()) == 0 {
            WinHttpCloseHandle(req);
            WinHttpCloseHandle(connect);
            WinHttpCloseHandle(session);
            return None;
        }

        let mut status: u32 = 0;
        let mut status_size = 4u32;
        WinHttpQueryHeaders(
            req,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            &mut status as *mut u32 as *mut c_void,
            &mut status_size,
            null_mut(),
        );

        let mut loc_buf = [0u16; 1024];
        let mut loc_len = (loc_buf.len() * 2) as u32;
        let location = if WinHttpQueryHeaders(
            req,
            WINHTTP_QUERY_LOCATION,
            null(),
            loc_buf.as_mut_ptr() as *mut c_void,
            &mut loc_len,
            null_mut(),
        ) != 0
        {
            String::from_utf16_lossy(&loc_buf[..(loc_len as usize / 2).min(loc_buf.len())])
        } else {
            String::new()
        };

        let mut body_bytes: Vec<u8> = Vec::new();
        loop {
            let mut chunk = [0u8; 4096];
            let mut read = 0u32;
            if WinHttpReadData(
                req,
                chunk.as_mut_ptr() as *mut c_void,
                chunk.len() as u32,
                &mut read,
            ) == 0
                || read == 0
            {
                break;
            }
            body_bytes.extend_from_slice(&chunk[..read as usize]);
            if body_bytes.len() > 200_000 {
                break;
            }
        }

        WinHttpCloseHandle(req);
        WinHttpCloseHandle(connect);
        WinHttpCloseHandle(session);

        Some(Resp {
            status: status as u16,
            location,
            body: String::from_utf8_lossy(&body_bytes).to_string(),
        })
    }
}

pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
