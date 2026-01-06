use anyhow::Result;
use reqwest::Client;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use sysinfo::System;

use crate::models::{Hero, LcuConnection, Settings};
use crate::utils::{CHAMP_FILE, IMG_DIR, VERSION_FILE};

pub async fn lcu_request(
    client: &Client,
    conn: &LcuConnection,
    method: &str,
    endpoint: &str,
    body: Option<serde_json::Value>,
) -> Result<serde_json::Value> {
    let url = format!("{}{}", conn.url, endpoint);
    let builder = match method {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "PATCH" => client.patch(&url),
        _ => client.get(&url),
    };
    let req = builder
        .header("Authorization", &conn.auth_header)
        .header("Accept", "application/json");
    let resp = if let Some(b) = body {
        req.json(&b).send().await?
    } else {
        req.send().await?
    };
    Ok(resp.json().await.unwrap_or(serde_json::json!({})))
}

pub fn find_lcu_process(sys: &System) -> Option<(String, String)> {
    for (_pid, process) in sys.processes() {
        if process.name().eq_ignore_ascii_case("LeagueClientUx.exe") {
            let cmd = process.cmd();
            let mut port = None;
            let mut token = None;
            for arg in cmd {
                let s = arg.to_string_lossy();
                if s.starts_with("--app-port=") {
                    port = Some(s.replace("--app-port=", ""));
                }
                if s.starts_with("--remoting-auth-token=") {
                    token = Some(s.replace("--remoting-auth-token=", ""));
                }
            }
            if let (Some(p), Some(t)) = (port, token) {
                return Some((p, t));
            }
        }
    }
    None
}

pub async fn update_data(client: &Client, shared_heroes: &Arc<Mutex<HashMap<i32, Hero>>>) {
    let _ = fs::create_dir_all(IMG_DIR);
    let mut version = "14.23.1".to_string();
    if let Ok(resp) = client
        .get("https://ddragon.leagueoflegends.com/api/versions.json")
        .send()
        .await
    {
        if let Ok(arr) = resp.json::<Vec<String>>().await {
            if let Some(v) = arr.first() {
                version = v.clone();
            }
        }
    }
    let local_ver = fs::read_to_string(VERSION_FILE).unwrap_or_default();
    if version != local_ver || !Path::new(CHAMP_FILE).exists() {
        let c_url = format!(
            "https://ddragon.leagueoflegends.com/cdn/{}/data/zh_CN/champion.json",
            version
        );
        if let Ok(resp) = client.get(&c_url).send().await {
            if let Ok(txt) = resp.text().await {
                let _ = fs::write(CHAMP_FILE, &txt);
                let _ = fs::write(VERSION_FILE, &version);
            }
        }
    }
    if let Ok(content) = fs::read_to_string(CHAMP_FILE) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(data) = v.get("data").and_then(|d| d.as_object()) {
                let mut map = shared_heroes.lock().unwrap();
                map.clear();
                for (_, val) in data {
                    let key = val
                        .get("key")
                        .and_then(|x| x.as_str())
                        .unwrap_or("0")
                        .parse::<i32>()
                        .unwrap_or(0);
                    let name = val
                        .get("name")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string();
                    let title = val
                        .get("title")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string();
                    let id_str = val
                        .get("id")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string();
                    let img = val
                        .get("image")
                        .and_then(|x| x.get("full"))
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string();
                    let alias = format!("{} {} {}", name, title, id_str).to_lowercase();
                    map.insert(
                        key,
                        Hero {
                            id: key,
                            name,
                            alias,
                            image_name: img,
                        },
                    );
                }
            }
        }
    }
}

pub async fn spoof_rank(client: &Client, conn: &LcuConnection, s: &Settings) {
    let payload = serde_json::json!({ "lol": { "rankedLeagueTier": s.spoof_tier, "rankedLeagueDivision": s.spoof_div, "rankedLeagueQueue": "RANKED_SOLO_5x5", "rankedLeagueJo": "RUBY" } });
    let _ = lcu_request(client, conn, "PUT", "/lol-chat/v1/me", Some(payload)).await;
}
