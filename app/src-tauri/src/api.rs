//! 后端接口访问层（对应原 ProgConsts / SettingsHelper / ServerMatchTask / SpeedServerConfig）

use crate::model::Node;
use serde::Deserialize;
use std::time::Duration;

pub const BASE: &str = "https://dlcv2.cnspeedtest.cn:8443";
pub const CDN: &str = "https://down.cnspeedtest.cn:8043";
pub const DES_KEY: &str = "dw!@#$%^";
pub const UA: &str = "GlobalSpeedTest-PC/1.0 (Windows)";

fn agent(timeout: u64) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(6))
        .timeout_read(Duration::from_secs(timeout))
        .timeout_write(Duration::from_secs(6))
        .user_agent(UA)
        .build()
}

pub fn get_text(url: &str, timeout: u64) -> Result<String, String> {
    match agent(timeout).get(url).call() {
        Ok(resp) => resp.into_string().map_err(|e| e.to_string()),
        Err(ureq::Error::Status(code, _)) => Err(format!("HTTP {}", code)),
        Err(e) => Err(e.to_string()),
    }
}

pub fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for b in input.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[derive(Deserialize, Debug, Clone)]
pub struct ConfigItem {
    pub name: String,
    pub filename: String,
    pub md5: String,
}

pub fn config_manifest(
    province: &str,
    city: &str,
    imei: &str,
    ipv6: bool,
) -> Result<Vec<ConfigItem>, String> {
    let url = format!(
        "{}/TaierAndroid/Config/ConfigMD5.php?province={}&city={}&imei={}&appname={}&vercode=40408&type=globalspeed&ipv6={}",
        BASE,
        percent_encode(province),
        percent_encode(city),
        percent_encode(imei),
        percent_encode("全球网测"),
        if ipv6 { 1 } else { 0 }
    );
    let text = get_text(&url, 10)?;
    serde_json::from_str::<Vec<ConfigItem>>(&text).map_err(|e| format!("config parse: {e}"))
}

#[derive(Deserialize, Debug)]
struct MatchNode {
    hostid: String,
    #[serde(default)]
    pname: String,
    #[serde(default)]
    city: String,
    port: String,
    #[serde(default)]
    hostname: String,
    hostip: String,
}

/// 节点匹配：GET /dataServer/mobilematch_many.php
pub fn match_servers(
    ip: &str,
    province: &str,
    city: &str,
    oper: &str,
    ipv6: bool,
) -> Result<Vec<Node>, String> {
    let url = format!(
        "{}/dataServer/mobilematch_many.php?ip={}&network=5&province={}&city={}&wifioper={}&mobileoperid=0&ipv6={}&model=Android&pkg=com.cnspeedtest.globalspeed",
        BASE,
        percent_encode(ip),
        percent_encode(province),
        percent_encode(city),
        percent_encode(oper),
        if ipv6 { 1 } else { 0 }
    );
    let text = get_text(&url, 12)?;
    let arr: Vec<MatchNode> =
        serde_json::from_str(&text).map_err(|e| format!("match parse: {e} :: {text}"))?;
    Ok(arr
        .into_iter()
        .map(|m| Node {
            cityid: String::new(),
            geoid: String::new(),
            hostid: m.hostid,
            hostip: m.hostip,
            hostname: m.hostname,
            location: m.city,
            oper: oper.to_string(),
            pname: m.pname,
            port: m.port,
        })
        .collect())
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    if bytes.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let val = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    };
    for chunk in bytes.chunks(2) {
        out.push((val(chunk[0])? << 4) | val(chunk[1])?);
    }
    Some(out)
}

/// DES/ECB/PKCS5 解密节点表（密钥 dw!@#$%^），输入为 hex 文本
pub fn decrypt_serverlist(hex_text: &str) -> Result<Vec<Node>, String> {
    use des::cipher::{BlockDecrypt, KeyInit};
    use des::Des;

    let data = hex_decode(hex_text.trim()).ok_or("节点表不是合法 hex")?;
    let cipher = Des::new_from_slice(DES_KEY.as_bytes()).map_err(|e| e.to_string())?;
    let mut out = data;
    if out.len() % 8 != 0 {
        return Err("密文长度不是 8 的倍数".into());
    }
    for chunk in out.chunks_mut(8) {
        let mut block = des::cipher::generic_array::GenericArray::clone_from_slice(chunk);
        cipher.decrypt_block(&mut block);
        chunk.copy_from_slice(&block);
    }
    if let Some(&pad) = out.last() {
        let pad = pad as usize;
        if pad > 0 && pad <= 8 && pad <= out.len() {
            out.truncate(out.len() - pad);
        }
    }
    let text = String::from_utf8_lossy(&out).to_string();
    serde_json::from_str::<Vec<Node>>(&text).map_err(|e| format!("节点表解析失败: {e}"))
}

/// 拉取并解密最新节点表；失败则返回 Err 以便保留内置表
pub fn fetch_serverlist() -> Result<Vec<Node>, String> {
    let manifest = config_manifest("北京", "北京", "TS0000000000000000", false)?;
    let item = manifest
        .iter()
        .find(|i| i.name == "serverlist_encrypt_url")
        .ok_or("配置清单中没有节点表")?;
    let url = if item.filename.starts_with("http") {
        item.filename.clone()
    } else {
        format!("{}/TaierAndroid/Config/{}", CDN, item.filename)
    };
    let text = get_text(&url, 25)?;
    decrypt_serverlist(&text)
}

/// 尽力获取公网 IP（用于节点匹配）
pub fn public_ip() -> Option<String> {
    for url in ["https://api.ipify.org", "https://ipv4.icanhazip.com"] {
        if let Ok(t) = get_text(url, 4) {
            let t = t.trim().to_string();
            if t.len() >= 7 && t.split('.').count() == 4 {
                return Some(t);
            }
        }
    }
    None
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct IpInfo {
    pub ip: String,
    pub country: String,
    pub province: String,
    pub city: String,
    pub isp: String,
    pub oper: String,
}

fn oper_of(isp: &str) -> String {
    let s = isp.replace(' ', "");
    if s.contains("电信") {
        "电信".into()
    } else if s.contains("联通") || s.contains("网通") {
        "联通".into()
    } else if s.contains("移动") {
        "移动".into()
    } else if s.contains("教育") || s.contains("CERNET") {
        "教育网".into()
    } else if s.contains("广电") {
        "广电网".into()
    } else {
        String::new()
    }
}

/// 公网出口 IP + 归属地/运营商（优先 ip-api，失败退回 ipify）
pub fn detect_ip_info() -> IpInfo {
    #[derive(Deserialize)]
    struct IpApi {
        #[serde(default)]
        status: String,
        #[serde(default)]
        country: String,
        #[serde(default, rename = "regionName")]
        region_name: String,
        #[serde(default)]
        city: String,
        #[serde(default)]
        isp: String,
        #[serde(default)]
        query: String,
    }
    if let Ok(text) = get_text("http://ip-api.com/json/?lang=zh-CN", 5) {
        if let Ok(v) = serde_json::from_str::<IpApi>(&text) {
            if v.status == "success" || !v.query.is_empty() {
                return IpInfo {
                    oper: oper_of(&v.isp),
                    ip: v.query,
                    country: v.country,
                    province: v.region_name,
                    city: v.city,
                    isp: v.isp,
                };
            }
        }
    }
    // 退路 1：cip.cc（纯文本）
    if let Ok(t) = get_text("https://cip.cc", 5) {
        let mut info = IpInfo::default();
        for line in t.lines() {
            let line = line.trim();
            if let Some(v) = line.strip_prefix("IP") {
                info.ip = v.trim_start_matches([':', ' ']).trim().to_string();
            } else if let Some(v) = line.strip_prefix("地址") {
                let addr = v.trim_start_matches([':', ' ']).trim().to_string();
                let parts: Vec<&str> = addr.split_whitespace().collect();
                if parts.len() >= 2 {
                    info.province = parts[parts.len() - 2].to_string();
                    info.city = parts[parts.len() - 1].to_string();
                }
            } else if let Some(v) = line.strip_prefix("运营商") {
                info.isp = v.trim_start_matches([':', ' ']).trim().to_string();
                info.oper = oper_of(&info.isp);
            }
        }
        if !info.ip.is_empty() {
            return info;
        }
    }
    // 退路 2：只要能拿到 IP
    if let Some(ip) = public_ip() {
        return IpInfo {
            ip,
            ..Default::default()
        };
    }
    IpInfo::default()
}

pub fn server_time() -> Option<u64> {
    get_text(&format!("{}/dataServer/time.php", BASE), 6)
        .ok()
        .and_then(|t| t.trim().parse::<u64>().ok())
}

#[derive(Deserialize, Debug, Clone)]
pub struct Evaluation {
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub code: i64,
}

pub fn evaluate(down: f64, up: f64, ping: f64) -> Option<Evaluation> {
    let url = format!(
        "{}/dataServer/resultEvaluation.php?down={:.2}&up={:.2}&ping={:.2}",
        BASE, down, up, ping
    );
    get_text(&url, 6)
        .ok()
        .and_then(|t| serde_json::from_str::<Evaluation>(&t).ok())
}
