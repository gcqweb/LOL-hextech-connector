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
            
            // 如果启用自动接受（暂停接管时不自动接受，但仍上报状态供手动取消）
            if settings.auto_accept && !settings.pause_all {
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
    if settings.pause_all {
        return;
    }
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
    if settings.pause_all {
        return;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 暂停接管时 lobby / end_of_game 自动化应早退：不产生消息、不触发网络、不改动状态。
    #[test]
    fn pause_all_skips_lobby_and_end_of_game() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = Client::new();
        let conn = LcuConnection {
            url: "https://127.0.0.1:1".into(),
            auth_header: "Basic eA==".into(),
        };
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut s = Settings::default();
        s.pause_all = true;
        s.auto_queue = true;
        s.auto_honor = true;
        s.play_again = true;
        let mut queue_timer = None;
        let mut honored = false;
        let mut played_again = false;
        rt.block_on(async {
            handle_lobby(&client, &conn, &s, &tx, &mut queue_timer).await;
            handle_end_of_game(&client, &conn, &s, &tx, &mut honored, &mut played_again).await;
        });
        assert!(rx.try_recv().is_err(), "暂停时不应产生任何自动化消息");
        assert!(queue_timer.is_none(), "暂停时不应启动匹配计时");
        assert!(!honored && !played_again, "暂停时不应执行点赞 / 回房间");
    }
}
