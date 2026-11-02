//! 本地持久化：设置与历史记录（对应原 PrefUtil / HistoryEntity）

use crate::model::{Settings, TestRecord};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

pub fn data_dir(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    let _ = fs::create_dir_all(&dir);
    dir
}

fn settings_path(app: &tauri::AppHandle) -> PathBuf {
    data_dir(app).join("settings.json")
}

fn history_path(app: &tauri::AppHandle) -> PathBuf {
    data_dir(app).join("history.json")
}

pub fn load_settings(app: &tauri::AppHandle) -> Settings {
    let p = settings_path(app);
    if let Ok(text) = fs::read_to_string(&p) {
        if let Ok(mut s) = serde_json::from_str::<Settings>(&text) {
            if s.device_id.is_empty() {
                s.device_id = format!("TS{}", crate::speed::rand_hex(16));
                let _ = save_settings(app, &s);
            }
            return s;
        }
    }
    let mut s = Settings::default();
    s.device_id = format!("TS{}", crate::speed::rand_hex(16));
    let _ = save_settings(app, &s);
    s
}

pub fn save_settings(app: &tauri::AppHandle, s: &Settings) -> Result<(), String> {
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    fs::write(settings_path(app), text).map_err(|e| e.to_string())
}

pub fn load_history(app: &tauri::AppHandle) -> Vec<TestRecord> {
    let p = history_path(app);
    if let Ok(text) = fs::read_to_string(&p) {
        if let Ok(v) = serde_json::from_str::<Vec<TestRecord>>(&text) {
            return v;
        }
    }
    Vec::new()
}

pub fn save_history(app: &tauri::AppHandle, v: &[TestRecord]) -> Result<(), String> {
    let text = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    fs::write(history_path(app), text).map_err(|e| e.to_string())
}

pub fn export_csv(app: &tauri::AppHandle, v: &[TestRecord]) -> Result<String, String> {
    let mut out = String::from(
        "时间,节点,运营商,省份,IP,端口,Ping(ms),抖动(ms),丢包(%),下行平均(Mbps),下行峰值(Mbps),上行平均(Mbps),上行峰值(Mbps),评分,评级\n",
    );
    for r in v {
        out.push_str(&format!(
            "{},{},{},{},{},{},{:.2},{:.2},{:.0},{:.2},{:.2},{:.2},{:.2},{},{}\n",
            r.time_text,
            r.node.hostname,
            r.node.oper,
            r.node.pname,
            r.node.hostip,
            r.node.port,
            r.ping.avg,
            r.ping.jitter,
            r.ping.loss,
            r.down.avg,
            r.down.max,
            r.up.avg,
            r.up.max,
            r.score,
            r.grade
        ));
    }
    let path = data_dir(app).join("history_export.csv");
    fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}
