//! 测速引擎（对应原 SpeedTestTask / SpeedUpDownTask / TcpConnection / Sampler / MyPingHelper）

use crate::model::{PingStat, Sample, SpeedPhase};
use md5::{Digest, Md5};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const UA_DL: &str = "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/100.0.4896.60 Safari/537.36";
pub const UA_UL: &str = "Dalvik/1.6.0 (Linux; U; Android 4.2.2; GT-I9505 Build/JDQ39)";
pub const SALT: &str = "12345555";

fn md5_hex(s: &str) -> String {
    let mut h = Md5::new();
    h.update(s.as_bytes());
    let d = h.finalize();
    let mut out = String::with_capacity(32);
    for b in d {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

/// 会话 token：md5( md5("model=..&imei=..") + md5("stime=..&band=..&rand=12345555") )
pub fn token(model: &str, imei: &str, t: u64, bw: u32) -> String {
    let b1 = format!("model={}&imei={}", model, imei);
    let b2 = format!("stime={}&band={}&rand={}", t, bw, SALT);
    md5_hex(&format!("{}{}", md5_hex(&b1), md5_hex(&b2)))
}

/// HTTP Host 头中的主机写法：IPv6 字面量要加方括号
pub fn host_header(host: &str) -> String {
    if host.contains(':') {
        format!("[{}]", host)
    } else {
        host.to_string()
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

struct Rng(u64);
impl Rng {
    fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15)
            ^ std::process::id() as u64;
        Rng(seed | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

pub fn rand_hex(n: usize) -> String {
    let mut r = Rng::new();
    let mut s = String::with_capacity(n);
    while s.len() < n {
        s.push_str(&format!("{:016X}", r.next()));
    }
    s.truncate(n);
    s
}

pub fn new_uuid_v4() -> String {
    let mut r = Rng::new();
    let a = r.next();
    let b = r.next();
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (a >> 32) as u32,
        ((a >> 16) & 0xffff) as u16,
        (a & 0x0fff) as u16,
        0x8000 | ((b >> 48) & 0x3fff) as u16,
        b & 0xffff_ffff_ffff
    )
}

pub struct SessionCfg<'a> {
    pub host: &'a str,
    pub port: u16,
    pub model: &'a str,
    pub imei: &'a str,
    pub bandwidth: u32,
    pub app: &'a str,
    pub pkg: &'a str,
}

fn http_get_host(host: &str, port: u16, path: &str, timeout: Duration) -> Result<String, String> {
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("DNS: {e}"))?
        .next()
        .ok_or("无可用地址")?;
    let mut s = TcpStream::connect_timeout(&addr, timeout).map_err(|e| format!("连接: {e}"))?;
    s.set_read_timeout(Some(timeout)).ok();
    s.set_write_timeout(Some(timeout)).ok();
    let req = format!(
        "GET {} HTTP/1.1\r\nAccept: */*\r\nConnection: close\r\nUser-Agent: {}\r\nHost: {}:{}\r\n\r\n",
        path, UA_DL, host_header(host), port
    );
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    loop {
        match s.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                if buf.len() > 65536 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let text = String::from_utf8_lossy(&buf).to_string();
    let (head, body) = match text.find("\r\n\r\n") {
        Some(pos) => (text[..pos].to_string(), text[pos + 4..].to_string()),
        None => (String::new(), text),
    };
    let chunked = head.to_ascii_lowercase().contains("transfer-encoding: chunked");
    if chunked {
        Ok(dechunk(&body))
    } else {
        Ok(body)
    }
}

/// 解析 chunked 传输体（服务端 /speed/dovalid 使用 Transfer-Encoding: chunked）
fn dechunk(body: &str) -> String {
    let bytes = body.as_bytes();
    let mut out = String::new();
    let mut i = 0usize;
    while i < bytes.len() {
        // 读取一行十六进制长度
        let line_end = match body[i..].find("\r\n") {
            Some(p) => i + p,
            None => break,
        };
        let size_txt = body[i..line_end].trim();
        let size = match usize::from_str_radix(size_txt.split(';').next().unwrap_or("0").trim(), 16) {
            Ok(v) => v,
            Err(_) => break,
        };
        if size == 0 {
            break;
        }
        let start = line_end + 2;
        let end = (start + size).min(bytes.len());
        out.push_str(&String::from_utf8_lossy(&bytes[start..end]));
        i = end + 2;
    }
    out
}

/// 建立测速会话：GET /speed/dovalid?...
pub fn session_open(cfg: &SessionCfg, timeout: Duration) -> Result<String, String> {
    let t = now_secs();
    let tok = token(cfg.model, cfg.imei, t, cfg.bandwidth);
    let path = format!(
        "/speed/dovalid?key=&flag=true&bandwidth={}&model={}&imei={}&time={}&app={}&token={}&pkg={}",
        cfg.bandwidth, cfg.model, cfg.imei, t, cfg.app, tok, cfg.pkg
    );
    let body = http_get_host(cfg.host, cfg.port, &path, timeout)?;
    let body = body.trim().to_string();
    if body.len() > 2 && body.starts_with('1') {
        Ok(body[2..].to_string())
    } else {
        Err(format!("服务器拒绝会话: {}", body))
    }
}

/// 结束会话：POST /speed/dovalid?key=uuid
pub fn session_close(host: &str, port: u16, uuid: &str) {
    let addr = match (host, port).to_socket_addrs().ok().and_then(|mut i| i.next()) {
        Some(a) => a,
        None => return,
    };
    if let Ok(mut s) = TcpStream::connect_timeout(&addr, Duration::from_secs(4)) {
        let req = format!(
            "POST /speed/dovalid?key={} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
            uuid, host_header(host), port
        );
        let _ = s.write_all(req.as_bytes());
        let _ = s.flush();
    }
}

pub fn dl_path(uuid: &str) -> String {
    format!("/speed/File(1G).dl?r={}&key={}", now_secs(), uuid)
}

pub const UL_PATH: &str = "/speed/doAnalsLoad.do";

fn dl_worker(host: String, port: u16, path: String, bytes: Arc<AtomicU64>, stop: Arc<AtomicBool>) {
    let addr = match (host.as_str(), port).to_socket_addrs().ok().and_then(|mut i| i.next()) {
        Some(a) => a,
        None => return,
    };
    let mut s = match TcpStream::connect_timeout(&addr, Duration::from_secs(4)) {
        Ok(s) => s,
        Err(_) => return,
    };
    s.set_read_timeout(Some(Duration::from_millis(1500))).ok();
    s.set_nodelay(true).ok();
    let req = format!(
        "GET {} HTTP/1.1\r\nAccept: */*\r\nConnection: close\r\nUser-Agent: {}\r\nHost: {}:{}\r\n\r\n",
        path, UA_DL, host_header(&host), port
    );
    if s.write_all(req.as_bytes()).is_err() {
        return;
    }
    let mut buf = vec![0u8; 64 * 1024];
    while !stop.load(Ordering::Relaxed) {
        match s.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                bytes.fetch_add(n as u64, Ordering::Relaxed);
            }
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(_) => break,
        }
    }
    let _ = s.shutdown(std::net::Shutdown::Both);
}

fn ul_worker(host: String, port: u16, uuid: String, bytes: Arc<AtomicU64>, stop: Arc<AtomicBool>) {
    let addr = match (host.as_str(), port).to_socket_addrs().ok().and_then(|mut i| i.next()) {
        Some(a) => a,
        None => return,
    };
    let mut s = match TcpStream::connect_timeout(&addr, Duration::from_secs(4)) {
        Ok(s) => s,
        Err(_) => return,
    };
    s.set_write_timeout(Some(Duration::from_millis(2000))).ok();
    s.set_nodelay(true).ok();
    let head = format!(
        "POST {} HTTP/1.1\r\nConnection: close\r\nCache-Control: no-cache\r\nCharset: UTF-8\r\nKey: {}\r\nContent-Type: multipart/form-data;boundary=00content0boundary00\r\nUser-Agent: {}\r\nHost: {}:{}\r\nAccept-Encoding: gzip\r\nContent-Length: 900000000\r\n\r\n--00content0boundary00\r\nContent-Disposition: form-data; name=\"upload\";filename=\"SPEED_{}\"\r\n\r\n",
        UL_PATH, uuid, UA_UL, host_header(&host), port, now_secs()
    );
    if s.write_all(head.as_bytes()).is_err() {
        return;
    }
    let mut buf = vec![0u8; 16 * 1024];
    let mut rng = Rng::new();
    for chunk in buf.chunks_mut(8) {
        chunk.copy_from_slice(&rng.next().to_le_bytes());
    }
    while !stop.load(Ordering::Relaxed) {
        match s.write_all(&buf) {
            Ok(()) => {
                bytes.fetch_add(buf.len() as u64, Ordering::Relaxed);
            }
            Err(_) => break,
        }
    }
    let _ = s.shutdown(std::net::Shutdown::Both);
}

/// 单向上/下行测试，返回采样与统计
pub struct TransferOutcome {
    pub phase: SpeedPhase,
    pub total_bytes: u64,
    pub elapsed_ms: u64,
}

#[allow(clippy::too_many_arguments)]
pub fn run_transfer(
    host: &str,
    port: u16,
    path: &str,
    uuid: &str,
    upload: bool,
    threads: u32,
    warmup_ms: u64,
    duration_ms: u64,
    mut on_sample: impl FnMut(Sample),
    cancel: Arc<AtomicBool>,
) -> TransferOutcome {
    let bytes = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let threads = threads.clamp(1, 64);

    let mut handles = Vec::new();
    for _ in 0..threads {
        let h = host.to_string();
        let u = uuid.to_string();
        let p = path.to_string();
        let b = bytes.clone();
        let st = stop.clone();
        handles.push(std::thread::spawn(move || {
            if upload {
                ul_worker(h, port, u, b, st)
            } else {
                dl_worker(h, port, p, b, st)
            }
        }));
    }

    let started = Instant::now();
    let interval = Duration::from_millis(500);
    let mut idx = 0u32;
    let mut phase = SpeedPhase::default();
    // (相对秒数, 累计字节)：用真实时间戳做 1 秒滑动窗口，避免"假设固定 500ms"的误差
    let mut hist: Vec<(f64, u64)> = Vec::new();
    let total_ms = warmup_ms + duration_ms;

    loop {
        std::thread::sleep(interval);
        let elapsed_s = started.elapsed().as_secs_f64();
        let elapsed = (elapsed_s * 1000.0) as u64;
        let total = bytes.load(Ordering::Relaxed);
        hist.push((elapsed_s, total));

        // 取窗口起点：尽量接近 elapsed-1.0s 的样本，没有就用最早的
        let cutoff = elapsed_s - 1.0;
        let base = hist
            .iter()
            .rev()
            .find(|(t, _)| *t <= cutoff)
            .copied()
            .unwrap_or(hist[0]);
        let span = (elapsed_s - base.0).max(0.2);
        let speed = total.saturating_sub(base.1) as f64 * 8.0 / span / 1_000_000.0;

        idx += 1;
        let preheat = elapsed <= warmup_ms;
        if !preheat {
            phase.samples.push((speed * 100.0).round() / 100.0);
            if speed > phase.max {
                phase.max = speed;
            }
        }
        on_sample(Sample {
            idx,
            speed: (speed * 100.0).round() / 100.0,
            bytes: total,
            preheat,
        });
        if elapsed >= total_ms || cancel.load(Ordering::Relaxed) {
            break;
        }
    }

    stop.store(true, Ordering::Relaxed);
    for h in handles {
        let _ = h.join();
    }
    // 等待残余读超时结束
    std::thread::sleep(Duration::from_millis(120));

    // 掐头去尾：预热点已在采集时剔除，这里再去掉最后一个 0.5s 采样
    // （测速停止瞬间那个窗口只有半个采样周期，会把平均值拉低）
    if !phase.samples.is_empty() {
        let mut core: &[f64] = &phase.samples;
        if core.len() > 4 {
            core = &core[..core.len() - 1];
        }
        let sum: f64 = core.iter().sum();
        phase.avg = sum / core.len() as f64;
    }
    TransferOutcome {
        phase,
        total_bytes: bytes.load(Ordering::Relaxed),
        elapsed_ms: started.elapsed().as_millis() as u64,
    }
}

/// TCP 握手 RTT（对应 MyPingHelper.do_tcpping）
pub fn tcp_rtt(host: &str, port: u16, count: u32, timeout_ms: u64) -> PingStat {
    let addr = match (host, port).to_socket_addrs().ok().and_then(|mut i| i.next()) {
        Some(a) => a,
        None => return PingStat::default(),
    };
    let mut ok = 0u32;
    let mut samples = Vec::new();
    for _ in 0..count.max(1) {
        let t0 = Instant::now();
        match TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms)) {
            Ok(s) => {
                let ms = t0.elapsed().as_secs_f64() * 1000.0;
                samples.push((ms * 100.0).round() / 100.0);
                ok += 1;
                drop(s);
            }
            Err(_) => {}
        }
        std::thread::sleep(Duration::from_millis(120));
    }
    let loss = 100.0 * (count.saturating_sub(ok)) as f64 / count.max(1) as f64;
    if samples.is_empty() {
        return PingStat {
            loss,
            ..Default::default()
        };
    }
    let avg = samples.iter().sum::<f64>() / samples.len() as f64;
    let min = samples.iter().cloned().fold(f64::MAX, f64::min);
    let max = samples.iter().cloned().fold(f64::MIN, f64::max);
    let jitter = if samples.len() > 1 {
        let mut sum = 0.0;
        for w in samples.windows(2) {
            sum += (w[1] - w[0]).abs();
        }
        sum / (samples.len() - 1) as f64
    } else {
        0.0
    };
    PingStat {
        avg: (avg * 100.0).round() / 100.0,
        min,
        max,
        jitter: (jitter * 100.0).round() / 100.0,
        loss,
        samples,
    }
}

/// DNS 解析耗时（ms）
pub fn dns_time(host: &str, port: u16) -> f64 {
    let t0 = Instant::now();
    let ok = (host, port).to_socket_addrs().is_ok();
    if ok {
        (t0.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0
    } else {
        -1.0
    }
}

/// 综合评分（0-100）：下行 45 分、上行 25 分、时延 30 分，速率按对数刻度
pub fn score_of(down: f64, up: f64, ping: f64) -> (u32, String) {
    let d = (45.0 * (1.0 + down.max(0.0)).log10() / 1001.0f64.log10()).min(45.0);
    let u = (25.0 * (1.0 + up.max(0.0)).log10() / 501.0f64.log10()).min(25.0);
    let p = (30.0 * (1.0 - (ping.max(0.0) / 150.0).min(1.0))).max(0.0);
    let score = (d + u + p).round().clamp(0.0, 100.0) as u32;
    let grade = match score {
        90..=100 => "极佳",
        75..=89 => "优秀",
        55..=74 => "良好",
        35..=54 => "一般",
        _ => "较差",
    };
    (score, grade.to_string())
}
