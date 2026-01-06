mod champ_select;
mod flow;

use base64::prelude::*;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sysinfo::{ProcessesToUpdate, System};

use crate::lcu::{find_lcu_process, lcu_request, spoof_rank, update_data};
use crate::models::{ActionState, BackendMsg, GuiMsg, Hero, LcuConnection};
use crate::utils::{load_settings, save_settings_to_disk};

use champ_select::handle_champ_select;
use flow::{handle_end_of_game, handle_lobby, handle_ready_check};

pub async fn run_backend(
    tx: crossbeam_channel::Sender<GuiMsg>,
    rx: crossbeam_channel::Receiver<BackendMsg>,
    shared_heroes: Arc<Mutex<HashMap<i32, Hero>>>,
) {
    let mut settings = load_settings();
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let mut connection: Option<LcuConnection> = None;

    // Logic State
    let mut last_phase = String::new();
    let mut honored = false;
    let mut played_again = false;
    let mut queue_timer: Option<Instant> = None;
    let mut last_bench_ids: Vec<i32> = Vec::new();

    // Action Tracking (ActionID -> State)
    let mut handled_actions: HashMap<i64, ActionState> = HashMap::new();

    tx.send(GuiMsg::LoadedData("检查数据...".into())).unwrap();
    update_data(&client, &shared_heroes).await;
    tx.send(GuiMsg::LoadedData("数据就绪".into())).unwrap();

    loop {
        // 1. Process Messages
        while let Ok(msg) = rx.try_recv() {
            match msg {
                BackendMsg::SaveSettings(s) => {
                    settings = s;
                    save_settings_to_disk(&settings);
                }
                BackendMsg::SwapChamp(id) => {
                    if let Some(conn) = &connection {
                        let _ = lcu_request(
                            &client,
                            conn,
                            "POST",
                            &format!("/lol-champ-select/v1/session/bench/swap/{}", id),
                            None,
                        )
                        .await;
                    }
                }
                BackendMsg::UpdateRank => {
                    if let Some(conn) = &connection {
                        spoof_rank(&client, conn, &settings).await;
                    }
                }
                BackendMsg::ForceReconnect => {
                    connection = None;
                    tx.send(GuiMsg::Status(false)).unwrap();
                    tx.send(GuiMsg::Log("正在重连...".into())).unwrap();
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }

        // 2. Connection Handling
        if connection.is_none() {
            // Re-initialize System to ensure we capture newly started processes correctly
            let mut sys = System::new_all();
            sys.refresh_processes(ProcessesToUpdate::All, true);
            match find_lcu_process(&sys) {
                Some((port, token)) => {
                    let auth = format!(
                        "Basic {}",
                        BASE64_STANDARD.encode(format!("riot:{}", token))
                    );
                    connection = Some(LcuConnection {
                        url: format!("https://127.0.0.1:{}", port),
                        auth_header: auth,
                    });
                    tx.send(GuiMsg::Log("已连接客户端".into())).unwrap();
                    tx.send(GuiMsg::Status(true)).unwrap();

                    // Reset State
                    last_phase = "None".into();
                    handled_actions.clear();

                    if settings.spoof_rank {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        spoof_rank(&client, connection.as_ref().unwrap(), &settings).await;
                    }
                }
                None => {
                    tx.send(GuiMsg::Status(false)).unwrap();
                    tokio::time::sleep(Duration::from_secs(1)).await; // Retry every 1s
                    continue;
                }
            }
        }

        let conn = connection.as_ref().unwrap().clone();

        // 3. Gameflow Phase
        let phase_val = match lcu_request(
            &client,
            &conn,
            "GET",
            "/lol-gameflow/v1/gameflow-phase",
            None,
        )
        .await
        {
            Ok(v) => v,
            Err(_) => {
                tx.send(GuiMsg::Log("连接断开".into())).unwrap();
                tx.send(GuiMsg::Status(false)).unwrap();
                connection = None;
                continue;
            }
        };

        let phase = phase_val.as_str().unwrap_or("None").to_string();
        if phase != last_phase {
            last_phase = phase.clone();
            tx.send(GuiMsg::Log(format!("状态: {}", phase))).unwrap();

            // Phase Change Reset - 进入新的 ChampSelect 时清除旧状态
            if phase == "ChampSelect" {
                handled_actions.clear();
            }
            if phase == "Lobby" {
                honored = false;
                played_again = false;
                queue_timer = None;
                if settings.spoof_rank {
                    spoof_rank(&client, &conn, &settings).await;
                }
            }
        }

        // 4. Phase Specific Logic
        let mut loop_delay = Duration::from_secs(2);

        match phase.as_str() {
            "ReadyCheck" => {
                loop_delay = Duration::from_millis(500);
                handle_ready_check(&client, &conn, &settings).await;
            }
            "ChampSelect" => {
                loop_delay = Duration::from_millis(200); // Fast tick for locking
                handle_champ_select(
                    &client,
                    &conn,
                    &settings,
                    &tx,
                    &mut last_bench_ids,
                    &mut handled_actions,
                    &shared_heroes,
                )
                .await;
            }
            "PreEndOfGame" | "EndOfGame" | "WaitingForStats" => {
                handle_end_of_game(
                    &client,
                    &conn,
                    &settings,
                    &tx,
                    &mut honored,
                    &mut played_again,
                )
                .await;
            }
            "Lobby" => {
                handle_lobby(&client, &conn, &settings, &tx, &mut queue_timer).await;
            }
            _ => {}
        }

        tokio::time::sleep(loop_delay).await;
    }
}
