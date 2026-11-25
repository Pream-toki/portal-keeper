use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Net {
    #[serde(default)]
    pub ssid: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub portal: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "check_default", rename = "checkSeconds")]
    pub check_seconds: u64,
    #[serde(default)]
    pub networks: Vec<Net>,
    #[serde(default, rename = "startWithWindows")]
    pub start_with_windows: bool,
}

fn yes() -> bool {
    true
}
fn check_default() -> u64 {
    15
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            check_seconds: 15,
            networks: vec![Net {
                ssid: "Your-WiFi-Name".into(),
                username: String::new(),
                password: String::new(),
                portal: String::new(),
            }],
            start_with_windows: true,
        }
    }
}

pub fn dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn path() -> PathBuf {
    dir().join("config.json")
}

pub fn log_path() -> PathBuf {
    dir().join("keeper.log")
}

pub fn load() -> Config {
    match std::fs::read_to_string(path()) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            log(&format!("config.json is not valid JSON: {e}"));
            Config::default()
        }),
        Err(_) => {
            let cfg = Config::default();
            save(&cfg);
            cfg
        }
    }
}

pub fn save(cfg: &Config) {
    if let Ok(text) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(path(), text + "\n");
    }
}

/// "KBU.FL7" == "KBU-FL7" == "kbu fl7" — SSIDs get typed with different punctuation
pub fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

pub fn lookup<'a>(cfg: &'a Config, ssid: &str) -> Option<&'a Net> {
    let want = normalize(ssid);
    cfg.networks.iter().find(|n| normalize(&n.ssid) == want)
}

pub fn log(msg: &str) {
    let p = log_path();
    if let Ok(m) = std::fs::metadata(&p) {
        if m.len() > 300_000 {
            let _ = std::fs::remove_file(&p);
        }
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        use std::io::Write;
        let _ = writeln!(f, "{} {}", stamp(), msg);
    }
}

fn stamp() -> String {
    let mut st = unsafe { std::mem::zeroed::<windows_sys::Win32::Foundation::SYSTEMTIME>() };
    unsafe { windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut st) };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
    )
}
