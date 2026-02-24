use reqwest::Client;
use std::time::{Duration, Instant};

use crate::lcu::lcu_request;
use crate::models::{GuiMsg, LcuConnection, Settings};

pub async fn handle_ready_check(
    client: &Client,
    conn: &LcuConnection,
    settings: &Settings,
    tx: &crossbeam_channel::Sender<GuiMsg>,
) {
    // 检查准备检查状态
    if let Ok(ready_check) = lcu_request(
        client,
        conn,
        "GET",
        "/lol-matchmaking/v1/ready-check",
        None,
    )
    .await
    {
        // 获取状态字段
        let state = ready_check.get("state").and_then(|s| s.as_str()).unwrap_or("");
        
        // 如果状态是 InProgress，通知 GUI 显示准备检查状态
        if state == "InProgress" {
            tx.send(GuiMsg::ReadyCheckState(true)).unwrap();
            
            // 如果启用自动接受
            if settings.auto_accept {
                let _ = lcu_request(
                    client,
                    conn,
                    "POST",
                    "/lol-matchmaking/v1/ready-check/accept",
                    None,
                )
                .await;
            }
        } else {
            // 准备检查结束，通知 GUI
            tx.send(GuiMsg::ReadyCheckState(false)).unwrap();
        }
    } else {
        // 请求失败，通知 GUI 准备检查已结束
        tx.send(GuiMsg::ReadyCheckState(false)).unwrap();
    }
}

/// 拒绝准备检查
pub async fn decline_ready_check(
    client: &Client,
    conn: &LcuConnection,
) {
    let _ = lcu_request(
        client,
        conn,
        "POST",
        "/lol-matchmaking/v1/ready-check/decline",
        None,
    )
    .await;
}

pub async fn handle_end_of_game(
    client: &Client,
    conn: &LcuConnection,
    settings: &Settings,
    tx: &crossbeam_channel::Sender<GuiMsg>,
    honored: &mut bool,
    played_again: &mut bool,
) {
    if settings.auto_honor && !*honored {
        if let Ok(ballot) =
            lcu_request(client, conn, "GET", "/lol-honor-v2/v1/ballot", None).await
        {
            if let Some(gid) = ballot.get("gameId") {
                let _ = lcu_request(
                    client,
                    conn,
                    "POST",
                    "/lol-honor-v2/v1/honor-player",
                    Some(serde_json::json!({"gameId": gid, "honorCategory": "OPT_OUT"})),
                )
                .await;
                *honored = true;
                tx.send(GuiMsg::Log("跳过点赞".into())).unwrap();
            }
        }
    }
    if settings.play_again && !*played_again {
        let _ = lcu_request(client, conn, "POST", "/lol-lobby/v2/play-again", None).await;
        *played_again = true;
        tx.send(GuiMsg::Log("返回房间".into())).unwrap();
    }
}

pub async fn handle_lobby(
    client: &Client,
    conn: &LcuConnection,
    settings: &Settings,
    tx: &crossbeam_channel::Sender<GuiMsg>,
    queue_timer: &mut Option<Instant>,
) {
    if settings.auto_queue {
        if queue_timer.is_none() {
            *queue_timer = Some(Instant::now());
            tx.send(GuiMsg::Log(format!("{}s后匹配", settings.queue_delay)))
                .unwrap();
        }
        if let Some(t) = queue_timer {
            if t.elapsed().as_secs() >= settings.queue_delay {
                if let Ok(st) = lcu_request(
                    client,
                    conn,
                    "GET",
                    "/lol-lobby/v2/lobby/matchmaking/search-state",
                    None,
                )
                .await
                {
                    if st.get("searchState").and_then(|s| s.as_str()).unwrap_or("") != "Searching" {
                        let _ = lcu_request(
                            client,
                            conn,
                            "POST",
                            "/lol-lobby/v2/lobby/matchmaking/search",
                            None,
                        )
                        .await;
                        tx.send(GuiMsg::Log("开始匹配".into())).unwrap();
                    }
                }
                *queue_timer = Some(Instant::now() + Duration::from_secs(9999));
            }
        }
    }
}
