use serde::{Deserialize, Serialize};

/// 测速节点（对应原 serverlist_encrypt.json 结构）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    #[serde(default)]
    pub cityid: String,
    #[serde(default)]
    pub geoid: String,
    pub hostid: String,
    pub hostip: String,
    pub hostname: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub oper: String,
    #[serde(default)]
    pub pname: String,
    pub port: String,
}

impl Node {
    pub fn port_num(&self) -> u16 {
        self.port.parse().unwrap_or(65499)
    }
    pub fn label(&self) -> String {
        if self.hostname.is_empty() {
            format!("{}:{}", self.hostip, self.port)
        } else {
            self.hostname.clone()
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub down_threads: u32,
    pub up_threads: u32,
    pub duration_sec: u32,
    pub bandwidth: u32,
    pub model: String,
    pub auto_match: bool,
    pub province: String,
    pub city: String,
    pub oper: String,
    pub device_id: String,
    #[serde(default)]
    pub last_server_id: String,
    /// 优先使用 IPv6 节点（原版 ProgConsts.IS_IPV6_MODE）
    #[serde(default)]
    pub ipv6: bool,
    /// 定时自动测速
    #[serde(default)]
    pub auto_enabled: bool,
    #[serde(default = "default_auto_interval")]
    pub auto_interval_min: u32,
}

fn default_auto_interval() -> u32 {
    30
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            down_threads: 8,
            up_threads: 4,
            duration_sec: 8,
            bandwidth: 200,
            model: "Android".into(),
            auto_match: true,
            province: "北京".into(),
            city: "北京".into(),
            oper: "电信".into(),
            device_id: String::new(),
            last_server_id: String::new(),
            ipv6: false,
            auto_enabled: false,
            auto_interval_min: 30,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sample {
    pub idx: u32,
    pub speed: f64,
    pub bytes: u64,
    pub preheat: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SpeedPhase {
    pub avg: f64,
    pub max: f64,
    pub samples: Vec<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct PingStat {
    pub avg: f64,
    pub min: f64,
    pub max: f64,
    pub jitter: f64,
    pub loss: f64,
    pub samples: Vec<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestRecord {
    pub id: String,
    pub ts: i64,
    pub time_text: String,
    pub node: Node,
    pub ping: PingStat,
    pub down: SpeedPhase,
    pub up: SpeedPhase,
    pub dns_ms: f64,
    pub connect_ms: f64,
    pub uuid: String,
    pub score: u32,
    pub grade: String,
    pub net: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestRequest {
    pub node: Option<Node>,
    pub ip: String,
    pub skip_upload: bool,
    pub skip_ping: bool,
}
