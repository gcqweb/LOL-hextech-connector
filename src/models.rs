use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// --- Constants ---
pub const TIER_MAP: [(&str, &str); 9] = [
    ("IRON", "坚韧黑铁"),
    ("BRONZE", "英勇黄铜"),
    ("SILVER", "不屈白银"),
    ("GOLD", "荣耀黄金"),
    ("PLATINUM", "华贵铂金"),
    ("EMERALD", "流光翡翠"),
    ("DIAMOND", "璀璨钻石"),
    ("MASTER", "超凡大师"),
    ("CHALLENGER", "最强王者"),
];

// --- Data Structures ---
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Settings {
    pub auto_accept: bool,
    pub auto_honor: bool,
    pub play_again: bool,
    pub auto_queue: bool,
    pub queue_delay: u64,
    pub spoof_rank: bool,
    pub spoof_tier: String,
    pub spoof_div: String,
    pub aram_snipe: bool,
    pub snipe_list: Vec<i32>,
    pub sr_enable: bool,
    pub sr_ban_enable: bool,
    pub auto_lock: bool,
    pub auto_ban_lock: bool,
    pub lock_time: u64,
    pub ban_time: u64,
    pub sr_picks: HashMap<String, i32>,
    pub sr_bans: HashMap<String, i32>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_accept: true,
            auto_honor: true,
            play_again: true,
            auto_queue: false,
            queue_delay: 3,
            spoof_rank: false,
            spoof_tier: "CHALLENGER".to_string(),
            spoof_div: "I".to_string(),
            aram_snipe: true,
            snipe_list: vec![],
            sr_enable: true,
            sr_ban_enable: true,
            auto_lock: false,
            auto_ban_lock: true,
            lock_time: 3,
            ban_time: 2,
            sr_picks: HashMap::new(),
            sr_bans: HashMap::new(),
        }
    }
}

#[derive(Clone)]
pub struct LcuConnection {
    pub url: String,
    pub auth_header: String,
}

pub enum GuiMsg {
    Log(String),
    Status(bool),
    BenchUpdate(Vec<i32>),
    LoadedData(String),
}

pub enum BackendMsg {
    SwapChamp(i32),
    SaveSettings(Settings),
    UpdateRank,
    ForceReconnect,
}

#[derive(Clone, Debug)]
pub struct Hero {
    pub id: i32,
    pub name: String,
    pub alias: String,
    pub image_name: String,
}

// --- Action State ---
#[derive(Clone, Default)]
pub struct ActionState {
    pub hovered: bool,
    pub completed: bool,
    pub last_act_time: Option<std::time::Instant>,
    /// 计划执行锁定的时间点（当 isInProgress 首次为 true 时设置）
    pub lock_scheduled_at: Option<std::time::Instant>,
}
