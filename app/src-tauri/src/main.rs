// 全球网测 桌面版 —— 基于对 Android 4.4.8 的逆向还原实现
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod model;
mod speed;
mod store;

use model::{Node, PingStat, Sample, Settings, TestRecord, TestRequest};
use serde::Serialize;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

pub struct AppCtx {
    settings: Mutex<Settings>,
    nodes: Mutex<Vec<Node>>,
    cancel: Mutex<Option<Arc<AtomicBool>>>,
    running: Arc<AtomicBool>,
}

const BUNDLED_NODES: &str = include_str!("../resources/nodes.json");

/// 会话上报的签约带宽（Mbps）。与原版 SpeedTestTask.setMobileBandwidth() 的 WiFi 分支保持一致：
/// network_class == 9（WiFi/以太网）→ 200；移动网分支为 150。PC 场景取 200。
const BANDWIDTH: u32 = 200;

#[derive(Serialize)]
struct Bootstrap {
    settings: Settings,
    nodes: Vec<Node>,
    history: Vec<TestRecord>,
    local_ip: String,
    data_dir: String,
    node_count: usize,
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn fmt_time(ts: i64) -> String {
    let days = ts.div_euclid(86400);
    let secs = ts.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        y,
        m,
        d,
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

fn local_ip() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("8.8.8.8:80")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

fn load_bundled_nodes() -> Vec<Node> {
    serde_json::from_str::<Vec<Node>>(BUNDLED_NODES).unwrap_or_default()
}

#[tauri::command]
fn bootstrap(app: AppHandle, ctx: State<AppCtx>) -> Bootstrap {
    let settings = ctx.settings.lock().unwrap().clone();
    let nodes = ctx.nodes.lock().unwrap().clone();
    Bootstrap {
        settings,
        history: store::load_history(&app),
        data_dir: store::data_dir(&app).to_string_lossy().to_string(),
        local_ip: local_ip(),
        node_count: nodes.len(),
        nodes,
    }
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    ctx: State<AppCtx>,
    settings: Settings,
) -> Result<Settings, String> {
    let mut cur = ctx.settings.lock().unwrap();
    let mut s = settings;
    if s.device_id.is_empty() {
        s.device_id = cur.device_id.clone();
    }
    *cur = s.clone();
    store::save_settings(&app, &s)?;
    Ok(s)
}

#[tauri::command]
async fn refresh_nodes(ctx: State<'_, AppCtx>) -> Result<usize, String> {
    let nodes = tauri::async_runtime::spawn_blocking(api::fetch_serverlist)
        .await
        .map_err(|e| e.to_string())??;
    if nodes.len() < 10 {
        return Err("节点表数量异常".into());
    }
    let n = nodes.len();
    *ctx.nodes.lock().unwrap() = nodes;
    Ok(n)
}

#[tauri::command]
async fn match_nodes(
    ip: String,
    province: String,
    city: String,
    oper: String,
) -> Result<Vec<Node>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        api::match_servers(&ip, &province, &city, &oper, false)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn detect_ip() -> api::IpInfo {
    tauri::async_runtime::spawn_blocking(api::detect_ip_info)
        .await
        .unwrap_or_default()
}

#[tauri::command]
async fn ipv6_available() -> bool {
    tauri::async_runtime::spawn_blocking(ipv6_available_blocking)
        .await
        .unwrap_or(false)
}

fn ipv6_available_blocking() -> bool {
    std::net::UdpSocket::bind("[::]:0")
        .and_then(|s| {
            s.connect("[2001:4860:4860::8888]:80")?;
            s.local_addr()
        })
        .map(|a| {
            let ip = a.ip();
            !ip.is_loopback() && !ip.is_unspecified()
        })
        .unwrap_or(false)
}

#[tauri::command]
async fn probe_node(host: String, port: u16) -> PingStat {
    tauri::async_runtime::spawn_blocking(move || speed::tcp_rtt(&host, port, 3, 1500))
        .await
        .unwrap_or_default()
}

/// 批量测延迟：内部 12 线程并发（原先在 JS 里串行 await，一个节点最多 1.5s）
#[tauri::command]
async fn probe_nodes_batch(items: Vec<(String, u16)>) -> Vec<(usize, f64)> {
    tauri::async_runtime::spawn_blocking(move || {
        use std::sync::atomic::{AtomicUsize, Ordering as O2};
        use std::sync::{Arc, Mutex};
        let n = items.len();
        let idx = Arc::new(AtomicUsize::new(0));
        let out = Arc::new(Mutex::new(Vec::with_capacity(n)));
        let workers = n.min(12).max(1);
        let mut hs = Vec::new();
        for _ in 0..workers {
            let items = items.clone();
            let idx = idx.clone();
            let out = out.clone();
            hs.push(std::thread::spawn(move || loop {
                let i = idx.fetch_add(1, O2::SeqCst);
                if i >= items.len() {
                    break;
                }
                let (host, port) = &items[i];
                let st = speed::tcp_rtt(host, *port, 2, 1000);
                // 回传输入下标，前端按下标对齐（避免 ip:port 与 ip|port 键格式不一致）
                out.lock().unwrap().push((i, st.avg));
            }));
        }
        for h in hs {
            let _ = h.join();
        }
        let guard = out.lock().unwrap();
        guard.clone()
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
async fn diag_ping(host: String, port: u16, count: u32) -> PingStat {
    tauri::async_runtime::spawn_blocking(move || speed::tcp_rtt(&host, port, count.clamp(1, 20), 2000))
        .await
        .unwrap_or_default()
}

#[tauri::command]
async fn diag_dns(host: String) -> serde_json::Value {
    tauri::async_runtime::spawn_blocking(move || diag_dns_blocking(host))
        .await
        .unwrap_or(serde_json::json!({}))
}

fn diag_dns_blocking(host: String) -> serde_json::Value {
    use std::net::ToSocketAddrs;
    let t0 = Instant::now();
    let addrs = (host.as_str(), 0u16)
        .to_socket_addrs()
        .map(|i| {
            let mut v: Vec<String> = i.map(|a| a.ip().to_string()).collect();
            v.sort();
            v.dedup();
            v
        })
        .unwrap_or_default();
    let ms = (t0.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0;
    json!({ "host": host, "ms": ms, "addrs": addrs })
}

#[tauri::command]
async fn diag_traceroute(host: String) -> String {
    tauri::async_runtime::spawn_blocking(move || diag_traceroute_blocking(host))
        .await
        .unwrap_or_else(|e| format!("执行失败: {e}"))
}

fn diag_traceroute_blocking(host: String) -> String {
    let _ = &host;
    let out = if cfg!(windows) {
        // tracert 输出是控制台 OEM 代码页（简中为 936），先用 chcp 65001 切成 UTF-8 再解析
        std::process::Command::new("cmd")
            .args(["/C", "chcp 65001>nul & tracert", "-d", "-w", "800", "-h", "24", &host])
            .output()
    } else {
        std::process::Command::new("traceroute")
            .args(["-n", "-w", "2", "-m", "24", &host])
            .output()
    };
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(e) => format!("执行失败: {e}"),
    }
}

/// 结果图输出目录：Windows → %USERPROFILE%\\Pictures；macOS/Linux → ~/Pictures
fn home_pictures_dir(app: &AppHandle) -> String {
    #[cfg(windows)]
    {
        if let Ok(u) = std::env::var("USERPROFILE") {
            return format!("{}/Pictures/GlobalSpeedTest", u.replace('\\', "/"));
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(h) = std::env::var("HOME") {
            return format!("{}/Pictures/GlobalSpeedTest", h);
        }
    }
    store::data_dir(app).to_string_lossy().to_string()
}

fn b64_decode(input: &str) -> Option<Vec<u8>> {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut map = [255u8; 256];
    for (i, c) in T.iter().enumerate() {
        map[*c as usize] = i as u8;
    }
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for c in input.bytes() {
        if c == b'=' || c == b'\n' || c == b'\r' {
            continue;
        }
        let v = *map.get(c as usize)?;
        if v == 255 {
            return None;
        }
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

/// 保存测速结果长图为 PNG（默认放到 图片\GlobalSpeedTest）
#[tauri::command]
async fn save_result_image(app: AppHandle, data_url: String, name: String) -> Result<String, String> {
    let payload = data_url
        .split(',')
        .next_back()
        .unwrap_or_default()
        .to_string();
    let path = tauri::async_runtime::spawn_blocking(move || {
        let bytes = b64_decode(&payload).ok_or("图片数据解析失败")?;
        let dir = home_pictures_dir(&app);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let p = format!("{}/{}.png", dir.trim_end_matches('/'), name);
        std::fs::write(&p, bytes).map_err(|e| e.to_string())?;
        Ok::<String, String>(p)
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(path)
}

/// 用系统默认程序打开文件（保存结果图后自动弹出）
#[tauri::command]
fn open_file(path: String) {
    let _ = path;
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", &path])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&path).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
    }
}

#[tauri::command]
fn history_list(app: AppHandle) -> Vec<TestRecord> {
    store::load_history(&app)
}

#[tauri::command]
fn history_delete(app: AppHandle, id: String) -> Vec<TestRecord> {
    let mut v = store::load_history(&app);
    v.retain(|r| r.id != id);
    let _ = store::save_history(&app, &v);
    v
}

#[tauri::command]
fn history_clear(app: AppHandle) -> Vec<TestRecord> {
    let _ = store::save_history(&app, &[]);
    Vec::new()
}

#[tauri::command]
fn history_export(app: AppHandle) -> Result<String, String> {
    let v = store::load_history(&app);
    store::export_csv(&app, &v)
}

#[tauri::command]
fn open_folder(path: String) {
    let target = if path.trim().is_empty() {
        ".".to_string()
    } else {
        path
    };
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer").arg(&target).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&target).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(&target).spawn();
    }
}

#[tauri::command]
fn stop_test(ctx: State<AppCtx>) {
    if let Some(c) = ctx.cancel.lock().unwrap().as_ref() {
        c.store(true, Ordering::Relaxed);
    }
}

#[tauri::command]
fn start_test(app: AppHandle, ctx: State<AppCtx>, req: TestRequest) -> Result<(), String> {
    if ctx.running.load(Ordering::Relaxed) {
        return Err("测速进行中".into());
    }
    let settings = ctx.settings.lock().unwrap().clone();
    let all_nodes = ctx.nodes.lock().unwrap().clone();
    let cancel = Arc::new(AtomicBool::new(false));
    *ctx.cancel.lock().unwrap() = Some(cancel.clone());
    ctx.running.store(true, Ordering::Relaxed);
    let running = ctx.running.clone();

    std::thread::spawn(move || {
        let res = run_test(&app, &settings, &req, &all_nodes, cancel.clone());
        if let Err(e) = res {
            let _ = app.emit("test-error", json!({ "message": e }));
        }
        running.store(false, Ordering::Relaxed);
    });
    Ok(())
}

fn gather_candidates(
    settings: &Settings,
    req: &TestRequest,
    all_nodes: &[Node],
    province: &str,
    city: &str,
    oper: &str,
    use_v6: bool,
) -> Vec<Node> {
    let mut c: Vec<Node> = Vec::new();
    if settings.auto_match {
        if let Ok(v) = api::match_servers(&req.ip, province, city, oper, use_v6) {
            c.extend(v);
        }
    }
    if c.is_empty() {
        let mut same: Vec<Node> = all_nodes
            .iter()
            .filter(|n| n.pname == province && (oper.is_empty() || n.oper == oper))
            .cloned()
            .collect();
        if same.is_empty() {
            same = all_nodes
                .iter()
                .filter(|n| n.pname == province)
                .cloned()
                .collect();
        }
        if same.is_empty() {
            same = all_nodes.to_vec();
        }
        c.extend(same);
    }
    c
}

fn score_nodes(list: &[Node]) -> Vec<(f64, Node)> {
    list.iter()
        .take(6)
        .map(|n| {
            let st = speed::tcp_rtt(&n.hostip, n.port_num(), 2, 1200);
            let rtt = if st.avg > 0.0 { st.avg } else { 9999.0 };
            (rtt, n.clone())
        })
        .collect()
}

fn pick_nodes(
    app: &AppHandle,
    settings: &Settings,
    req: &TestRequest,
    all_nodes: &[Node],
) -> Result<Vec<Node>, String> {
    if let Some(n) = &req.node {
        return Ok(vec![n.clone()]);
    }
    let _ = app.emit("phase", json!({ "phase": "match" }));

    let province = if settings.province.is_empty() {
        "北京"
    } else {
        settings.province.as_str()
    };
    let city = if settings.city.is_empty() {
        province
    } else {
        settings.city.as_str()
    };
    let oper = settings.oper.as_str();

    let mut scored = score_nodes(&gather_candidates(
        settings, req, all_nodes, province, city, oper, settings.ipv6,
    ));
    // IPv6 全部不可达时回退 IPv4，避免用户开了 v6 开关却没 v6 出口时报错
    let all_dead = !scored.is_empty() && scored.iter().all(|(r, _)| *r >= 9999.0);
    if all_dead && settings.ipv6 {
        let _ = app.emit(
            "phase",
            json!({ "phase": "match", "note": "IPv6 节点不可达，回退 IPv4" }),
        );
        scored = score_nodes(&gather_candidates(
            settings, req, all_nodes, province, city, oper, false,
        ));
    }
    if scored.is_empty() {
        return Err("没有可用测速节点".into());
    }
    scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let ranked: Vec<Node> = scored.into_iter().map(|(_, n)| n).collect();
    let _ = app.emit(
        "phase",
        json!({ "phase": "match", "note": format!("{} · 候选 {} 个", ranked[0].hostname, ranked.len()) }),
    );
    Ok(ranked)
}

fn run_test(
    app: &AppHandle,
    settings: &Settings,
    req: &TestRequest,
    all_nodes: &[Node],
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let candidates = pick_nodes(app, settings, req, all_nodes)?;
    let _ = app.emit("node", json!({ "node": candidates[0] }));

    // ---- 1. 建立会话（首选节点失败自动切换备选）----
    let mut chosen: Option<(Node, String)> = None;
    let mut last_err = String::from("无候选节点");
    for (i, n) in candidates.iter().take(3).enumerate() {
        if cancel.load(Ordering::Relaxed) {
            let _ = app.emit("phase", json!({ "phase": "canceled" }));
            return Ok(());
        }
        if i > 0 {
            let _ = app.emit(
                "phase",
                json!({ "phase": "match", "note": format!("切换到备选节点 {}", n.hostname) }),
            );
            let _ = app.emit("node", json!({ "node": n }));
        }
        let cfg = speed::SessionCfg {
            host: &n.hostip,
            port: n.port_num(),
            model: &settings.model,
            imei: &settings.device_id,
            bandwidth: BANDWIDTH,
            app: "globalspeed",
            pkg: "com.cnspeedtest.globalspeed",
        };
        match speed::session_open(&cfg, Duration::from_secs(8)) {
            Ok(uuid) => {
                chosen = Some((n.clone(), uuid));
                break;
            }
            Err(e) => {
                last_err = format!("{}: {}", n.hostname, e);
            }
        }
    }
    let (node, uuid) = match chosen {
        Some(v) => v,
        None => return Err(format!("候选节点均失败（{}）", last_err)),
    };
    let host = node.hostip.clone();
    let port = node.port_num();
    let _ = app.emit("node", json!({ "node": node }));

    // ---- 2. DNS / 建连 / Ping ----
    let _ = app.emit("phase", json!({ "phase": "ping" }));
    let dns_ms = speed::dns_time(&host, port);
    let connect_rtt = speed::tcp_rtt(&host, port, 1, 1500);
    let mut ping = if req.skip_ping {
        PingStat::default()
    } else {
        speed::tcp_rtt(&host, port, 5, 2000)
    };
    if ping.avg <= 0.0 {
        ping.avg = connect_rtt.avg;
    }
    let _ = app.emit("ping", json!({ "ping": ping, "dns_ms": dns_ms }));
    if cancel.load(Ordering::Relaxed) {
        let _ = app.emit("phase", json!({ "phase": "canceled" }));
        return Ok(());
    }

    // ---- 3. 下行 ----
    let _ = app.emit("phase", json!({ "phase": "download" }));
    let warmup_ms = 2000u64;
    let duration_ms = (settings.duration_sec.max(2) as u64) * 1000;
    let path = speed::dl_path(&uuid);
    let dl = speed::run_transfer(
        &host,
        port,
        &path,
        &uuid,
        false,
        settings.down_threads,
        warmup_ms,
        duration_ms,
        |s: Sample| {
            let _ = app.emit("sample", json!({ "dir": "down", "data": s }));
        },
        cancel.clone(),
    );
    let _ = app.emit(
        "phase-result",
        json!({ "dir": "down", "avg": dl.phase.avg, "max": dl.phase.max, "bytes": dl.total_bytes }),
    );

    // ---- 4. 上行 ----
    let mut up = model::SpeedPhase::default();
    if !req.skip_upload && !cancel.load(Ordering::Relaxed) {
        let _ = app.emit("phase", json!({ "phase": "upload" }));
        let u = speed::run_transfer(
            &host,
            port,
            speed::UL_PATH,
            &uuid,
            true,
            settings.up_threads,
            warmup_ms,
            duration_ms,
            |s: Sample| {
                let _ = app.emit("sample", json!({ "dir": "up", "data": s }));
            },
            cancel.clone(),
        );
        up = u.phase;
        let _ = app.emit(
            "phase-result",
            json!({ "dir": "up", "avg": up.avg, "max": up.max, "bytes": u.total_bytes }),
        );
    }

    // ---- 5. 关闭会话 ----
    speed::session_close(&host, port, &uuid);

    let (score, grade) = speed::score_of(dl.phase.avg, up.avg, ping.avg);
    let ts = speed::now_secs() as i64;
    let rec = TestRecord {
        id: format!("{}-{}", ts, speed::rand_hex(6)),
        ts,
        time_text: fmt_time(ts),
        node: node.clone(),
        ping: ping.clone(),
        down: dl.phase.clone(),
        up: up.clone(),
        dns_ms,
        connect_ms: ping.avg,
        uuid: uuid.clone(),
        score,
        grade: grade.clone(),
        net: local_ip(),
    };

    let mut hist = store::load_history(app);
    hist.insert(0, rec.clone());
    hist.truncate(500);
    let _ = store::save_history(app, &hist);

    let _ = app.emit(
        "done",
        json!({
            "record": rec,
            "ping": ping,
            "down": dl.phase,
            "up": up,
            "score": score,
            "grade": grade,
            "node": node,
            "uuid": uuid,
        }),
    );
    Ok(())
}

/// 命令行自检：不依赖 GUI，直接跑一次完整测速并打印结果。
/// 用法：globalspeed.exe --selftest [节点IP] [端口] [秒数]
fn cli_selftest() {
    // 位置参数：<ip> [port] [secs]（自动忽略 --selftest 等开关）
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();
    let all = load_bundled_nodes();
    let settings = Settings {
        device_id: format!("TS{}", speed::rand_hex(16)),
        ..Settings::default()
    };

    let mut node = if !args.is_empty() {
        Node {
            cityid: String::new(),
            geoid: String::new(),
            hostid: "cli".into(),
            hostip: args[0].clone(),
            hostname: args[0].clone(),
            location: String::new(),
            oper: "电信".into(),
            pname: String::new(),
            port: args.get(1).cloned().unwrap_or_else(|| "65499".into()),
        }
    } else {
        all.iter()
            .find(|n| n.oper == "电信" && n.pname == "北京")
            .cloned()
            .or_else(|| all.first().cloned())
            .expect("内置节点表为空")
    };

    let mut dur = args.get(2).and_then(|s| s.parse::<u64>().ok()).unwrap_or(6);
    if dur < 3 {
        dur = 3;
    }

    println!("== 全球网测 桌面版 自检 ==");
    println!("设备标识 : {}", settings.device_id);
    if args.is_empty() {
        println!("自动挑选可用节点…");
        let mut best = node.clone();
        let mut best_rtt = f64::MAX;
        for n in all.iter().take(40) {
            let st = speed::tcp_rtt(&n.hostip, n.port_num(), 1, 800);
            if st.avg > 0.0 && st.avg < best_rtt {
                best_rtt = st.avg;
                best = n.clone();
            }
            if best_rtt < 8.0 {
                break;
            }
        }
        node = best;
    }
    println!("节点     : {} ({}:{})", node.hostname, node.hostip, node.port);
    let host = node.hostip.clone();
    let port = node.port_num();

    println!("DNS      : {:.2} ms", speed::dns_time(&host, port));
    let ping = speed::tcp_rtt(&host, port, 5, 2000);
    println!(
        "Ping     : avg {:.2} ms  min {:.2}  max {:.2}  jitter {:.2}  loss {:.0}%",
        ping.avg, ping.min, ping.max, ping.jitter, ping.loss
    );

    let cfg = speed::SessionCfg {
        host: &host,
        port,
        model: &settings.model,
        imei: &settings.device_id,
        bandwidth: BANDWIDTH,
        app: "globalspeed",
        pkg: "com.cnspeedtest.globalspeed",
    };
    let uuid = match speed::session_open(&cfg, Duration::from_secs(8)) {
        Ok(u) => u,
        Err(e) => {
            println!("会话失败 : {e}");
            std::process::exit(2);
        }
    };
    println!("会话 UUID: {uuid}");

    let nocancel = Arc::new(AtomicBool::new(false));
    let dl = speed::run_transfer(
        &host,
        port,
        &speed::dl_path(&uuid),
        &uuid,
        false,
        settings.down_threads,
        1500,
        dur * 1000,
        |_s| {},
        nocancel.clone(),
    );
    println!(
        "下行     : 平均 {:.2} Mbps  峰值 {:.2} Mbps  传输 {:.1} MB",
        dl.phase.avg,
        dl.phase.max,
        dl.total_bytes as f64 / 1_048_576.0
    );

    let up = speed::run_transfer(
        &host,
        port,
        speed::UL_PATH,
        &uuid,
        true,
        settings.up_threads,
        1500,
        dur * 1000,
        |_s| {},
        nocancel.clone(),
    );
    println!(
        "上行     : 平均 {:.2} Mbps  峰值 {:.2} Mbps  传输 {:.1} MB",
        up.phase.avg,
        up.phase.max,
        up.total_bytes as f64 / 1_048_576.0
    );

    speed::session_close(&host, port, &uuid);
    let (score, grade) = speed::score_of(dl.phase.avg, up.phase.avg, ping.avg);
    println!("评分     : {score} · {grade}");
    println!("== 自检完成 ==");
}

fn main() {
    if std::env::args().any(|a| a == "--selftest") {
        cli_selftest();
        return;
    }
    let nodes = load_bundled_nodes();
    tauri::Builder::default()
        .manage(AppCtx {
            settings: Mutex::new(Settings::default()),
            nodes: Mutex::new(nodes),
            cancel: Mutex::new(None),
            running: Arc::new(AtomicBool::new(false)),
        })
        .setup(|app| {
            let handle = app.handle().clone();
            let ctx = handle.state::<AppCtx>();
            let s = store::load_settings(&handle);
            *ctx.settings.lock().unwrap() = s;
            let _ = handle.emit("ready", json!({ "ok": true }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            detect_ip,
            ipv6_available,
            save_settings,
            refresh_nodes,
            match_nodes,
            probe_node,
            probe_nodes_batch,
            diag_ping,
            diag_dns,
            diag_traceroute,
            history_list,
            history_delete,
            history_clear,
            history_export,
            save_result_image,
            open_folder,
            open_file,
            start_test,
            stop_test
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
