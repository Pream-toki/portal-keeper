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
    // Right after associating, DHCP may not have handed out a gateway yet.
    let mut found = None;
    for _ in 0..12 {
        if let Some(g) = wlan::gateway() {
            found = Some(g);
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let gateway = found.ok_or("no default gateway")?;
    let user = http::encode(net.username.trim());
    let pass = http::encode(&net.password);
    let dst = http::encode(PROBE);
    let mut last = String::from("no portal answered");

    let mut targets: Vec<(String, String)> = Vec::new();
    if !net.portal.trim().is_empty() {
        targets.push((
            net.portal.trim().to_string(),
            format!("username={user}&password={pass}&dst={dst}&popup=true"),
        ));
    }
    // MikroTik hotspot (KBU.WiFi)
    targets.push((
        format!("http://{gateway}/login"),
        format!("username={user}&password={pass}&dst={dst}&popup=true"),
    ));
    // IBSG Lite portal (KBU-FL7)
    targets.push((
        format!("http://{gateway}/portal/user-authen.php"),
        format!("txtLogin={user}&txtPasswd={pass}&btnLogin=Login&reqUrl={dst}&reqCheck=false"),
    ));

    for (url, body) in targets {
        let resp = match http::post(&url, &body, 8000) {
            Some(r) => r,
            None => {
                last = format!("{url} unreachable");
                continue;
            }
        };
        let auth = format!("status {}", resp.status);
        config::log(&format!("post {url} as {} -> {auth}", net.username));
        if !resp.location.is_empty() {
            let _ = http::get(&resp.location, 6000);
        }
        std::thread::sleep(Duration::from_millis(400));
        if online() {
            return Ok(());
        }
        last = format!("{url} -> {auth}");
    }
    Err(last)
}

/// Switch to another known network when this one is weak and the other is clearly better.
fn maybe_switch(cfg: &config::Config, current_ssid: &str) {
    let cur = match wlan::current() {
        Some(c) => c,
        None => return,
    };
    if cur.signal >= cfg.weak_signal {
        return;
    }
    let best = wlan::available()
        .into_iter()
        .filter(|w| {
            config::normalize(&w.ssid) != config::normalize(current_ssid)
                && config::lookup(cfg, &w.ssid).is_some()
                && w.signal > cur.signal + 10
        })
        .max_by_key(|w| w.signal);
    if let Some(b) = best {
        config::log(&format!(
            "auto-switch {} ({}%) -> {} ({}%)",
            cur.ssid, cur.signal, b.ssid, b.signal
        ));
        wlan::connect(&b.ssid);
    }
}

fn write_status(text: &str) {
    let _ = std::fs::write(config::dir().join("portal-keeper-status.txt"), text);
    println!("{text}");
}

fn cli(args: &[String]) -> bool {
    match args.get(1).map(|s| s.as_str()) {
        Some("--status") => {
            let cfg = config::load();
            let cur = wlan::current();
            let mut text = String::new();
            text += &format!(
                "wifi: {}\nsignal: {}\ngateway: {}\nonline: {}\nautoSwitch: {}\n",
                cur.as_ref().map(|w| w.ssid.clone()).unwrap_or_default(),
                cur.as_ref().map(|w| w.signal.to_string()).unwrap_or_default(),
                wlan::gateway().unwrap_or_default(),
                online(),
                cfg.auto_switch
            );
            text += "known:\n";
            for n in &cfg.networks {
                text += &format!("  {} -> {}\n", n.ssid, n.username);
            }
            text += "visible:\n";
            for w in wlan::available() {
                text += &format!("  {} {}%\n", w.ssid, w.signal);
            }
            write_status(&text);
            true
        }
        Some("--switch") => {
            let target = args.get(2).cloned().unwrap_or_default();
            let requested = !target.is_empty() && wlan::connect(&target);
            let joined = requested && wlan::wait_for(&target, 20);
            write_status(&format!(
                "switch {target}: {}\nwifi now: {}\nonline: {}\n",
                if joined {
                    "joined"
                } else if requested {
                    "requested, did not associate"
                } else {
                    "request failed"
                },
                wlan::ssid().unwrap_or_default(),
                online()
            ));
            true
        }
        _ => false,
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
                            if cfg.auto_switch {
                                maybe_switch(&cfg, &ssid);
                            }
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
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && cli(&args) {
        return;
    }
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
