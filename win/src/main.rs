#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod http;
mod tray;
mod wlan;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub struct Status {
    pub enabled: bool,
    pub ssid: String,
    pub msg: String,
    pub ok: bool,
}

static STATUS: OnceLock<Mutex<Status>> = OnceLock::new();
pub static FORCE: AtomicBool = AtomicBool::new(false);
pub static QUIT: AtomicBool = AtomicBool::new(false);

pub fn status() -> &'static Mutex<Status> {
    STATUS.get_or_init(|| {
        Mutex::new(Status {
            enabled: true,
            ssid: String::new(),
            msg: "starting".into(),
            ok: false,
        })
    })
}

pub fn snapshot() -> (bool, String, String, bool) {
    let s = status().lock().unwrap();
    (s.enabled, s.ssid.clone(), s.msg.clone(), s.ok)
}

fn set(msg: &str, ok: bool, ssid: &str) {
    let mut s = status().lock().unwrap();
    s.msg = msg.to_string();
    s.ok = ok;
    if !ssid.is_empty() {
        s.ssid = ssid.to_string();
    }
}

const PROBE: &str = "http://connectivitycheck.gstatic.com/generate_204";

fn online() -> bool {
    match http::get(PROBE, 3000) {
        Some(r) => r.status == 204 || r.status == 200,
        None => false,
    }
}

fn login(net: &config::Net) -> Result<(), String> {
    let gateway = wlan::gateway().ok_or("no default gateway")?;
    let portal = if net.portal.trim().is_empty() {
        format!("http://{gateway}/login")
    } else {
        net.portal.trim().to_string()
    };
    let body = format!(
        "username={}&password={}&dst={}&popup=true",
        http::encode(&net.username),
        http::encode(&net.password),
        http::encode(PROBE)
    );
    let resp = http::post(&portal, &body, 8000).ok_or("portal unreachable")?;
    config::log(&format!(
        "post {portal} as {} -> {} {}",
        net.username,
        resp.status,
        resp.location
    ));
    if resp.status >= 400 {
        return Err(format!("portal returned {}", resp.status));
    }
    if !resp.location.is_empty() {
        if let Some(follow) = http::get(&resp.location, 6000) {
            config::log(&format!("get {} -> {}", resp.location, follow.status));
        }
    }
    std::thread::sleep(Duration::from_millis(300));
    if online() {
        Ok(())
    } else {
        let body = resp.body.replace('\n', " ");
        let snippet: String = body.chars().take(200).collect();
        Err(format!("still offline after post ({}): {snippet}", resp.status))
    }
}

fn worker() {
    let mut last_ssid = String::new();
    let mut last_probe = Instant::now()
        .checked_sub(Duration::from_secs(600))
        .unwrap_or_else(Instant::now);

    loop {
        if QUIT.load(Ordering::SeqCst) {
            return;
        }
        let cfg = config::load();
        let ssid = wlan::ssid().unwrap_or_default();
        {
            let mut s = status().lock().unwrap();
            s.enabled = cfg.enabled;
            s.ssid = ssid.clone();
        }

        if !cfg.enabled {
            set("paused", false, &ssid);
        } else if ssid.is_empty() {
            set("no wi-fi", false, "");
        } else {
            match config::lookup(&cfg, &ssid) {
                None => set(&format!("idle · {ssid}"), false, &ssid),
                Some(net) if net.username.trim().is_empty() || net.password.is_empty() => {
                    set(&format!("no account · {ssid}"), false, &ssid)
                }
                Some(net) => {
                    let switched = config::normalize(&ssid) != config::normalize(&last_ssid);
                    let forced = FORCE.swap(false, Ordering::SeqCst);
                    if switched || forced || last_probe.elapsed() >= Duration::from_secs(cfg.check_seconds.max(5)) {
                        last_probe = Instant::now();
                        last_ssid = ssid.clone();
                        if online() {
                            set(&format!("ok · {ssid}"), true, &ssid);
                        } else {
                            set(&format!("signing in · {ssid}"), false, &ssid);
                            match login(net) {
                                Ok(()) => {
                                    config::log(&format!("signed in on {ssid}"));
                                    set(&format!("signed in · {ssid}"), true, &ssid);
                                }
                                Err(e) => {
                                    config::log(&format!("sign-in failed on {ssid}: {e}"));
                                    set(&format!("retrying · {ssid}"), false, &ssid);
                                    last_probe = Instant::now()
                                        .checked_sub(Duration::from_secs(cfg.check_seconds.saturating_sub(3)))
                                        .unwrap_or_else(Instant::now);
                                }
                            }
                        }
                        tray::refresh();
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(1500));
    }
}

pub fn startup_set(want: bool) {
    let exe = std::env::current_exe().unwrap_or_default();
    let value = format!("\"{}\"", exe.display());
    let mut cmd = std::process::Command::new("reg");
    if want {
        cmd.args([
            "add",
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
            "/v",
            "PortalKeeper",
            "/t",
            "REG_SZ",
            "/d",
            &value,
            "/f",
        ]);
    } else {
        cmd.args([
            "delete",
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
            "/v",
            "PortalKeeper",
            "/f",
        ]);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let _ = cmd.status();
}

fn main() {
    let cfg = config::load();
    if cfg.start_with_windows {
        startup_set(true);
    }
    {
        let mut s = status().lock().unwrap();
        s.enabled = cfg.enabled;
    }
    std::thread::spawn(worker);
    tray::run();
}
