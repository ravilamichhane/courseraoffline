use std::path::PathBuf;

pub mod admin;
pub mod auth;
pub mod courseeraclient;
pub mod courseeraresponses;
pub mod coursera;
pub mod database;
pub mod downloads;
pub mod frontend;
#[path = "generated/buffa/mod.rs"]
pub mod proto;
#[path = "generated/connect/mod.rs"]
pub mod services;
#[cfg(test)]
mod test_support;

pub fn env_var(name: &str) -> Option<String> {
    if let Ok(val) = std::env::var(name) {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Ok(dotenv) = std::fs::read_to_string(".env") {
        for line in dotenv.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                if k.trim() == name {
                    let val = v.trim().trim_matches('"').trim_matches('\'').trim();
                    if !val.is_empty() {
                        return Some(val.to_string());
                    }
                }
            }
        }
    }
    None
}

pub fn data_dir() -> PathBuf {
    env_var("DATA_DIR")
        .or_else(|| env_var("DATA_FOLDER"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("data")
        })
}

pub fn downloads_dir() -> PathBuf {
    data_dir().join("downloads")
}

pub fn database_path() -> PathBuf {
    data_dir().join("courses.db")
}

pub fn cookie_path() -> PathBuf {
    data_dir().join("cookie.txt")
}

pub fn api_key() -> Option<String> {
    env_var("API_KEY")
}

pub fn is_dev_mode() -> bool {
    env_var("MODE")
        .or_else(|| env_var("APP_ENV"))
        .is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "development" | "dev"))
}
