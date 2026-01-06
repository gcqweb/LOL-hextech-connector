use reqwest::Client;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::lcu::lcu_request;
use crate::models::{ActionState, GuiMsg, Hero, LcuConnection, Settings};
use crate::utils::lookup_hero_name_by_id;

/// 英雄选择阶段的主处理函数
/// 处理大乱斗板凳席和峡谷/排位的 Ban & Pick 逻辑
pub async fn handle_champ_select(
    client: &Client,
    conn: &LcuConnection,
    settings: &Settings,
    tx: &crossbeam_channel::Sender<GuiMsg>,
    last_bench_ids: &mut Vec<i32>,
    handled_actions: &mut HashMap<i64, ActionState>,
    shared_heroes: &Arc<Mutex<HashMap<i32, Hero>>>,
) {
    let session_json =
        match lcu_request(client, conn, "GET", "/lol-champ-select/v1/session", None).await {
            Ok(v) => v,
            Err(_) => return,
        };

    let local_cell_id = session_json
        .get("localPlayerCellId")
        .and_then(|v| v.as_i64())
        .unwrap_or(-1);
    if local_cell_id == -1 {
        return;
    }

    // --- 大乱斗板凳席模式 ---
    if session_json.get("benchEnabled").and_then(|v| v.as_bool()).unwrap_or(false) {
        handle_aram_bench(client, conn, settings, tx, session_json, last_bench_ids, local_cell_id, shared_heroes).await;
        return;
    }

    // --- 峡谷/排位 Ban & Pick 逻辑 ---
    handle_sr_pick_ban(client, conn, settings, tx, session_json, handled_actions, shared_heroes, local_cell_id).await;
}

/// 峡谷/排位的 Ban & Pick 核心逻辑
/// 
/// 关键设计：使用定时锁定机制
/// - 当 isInProgress 首次变为 true 且 timer.phase 正确时，计算锁定时间点
/// - 后续轮询检查是否到达锁定时间点，到达则执行锁定
async fn handle_sr_pick_ban(
    client: &Client,
    conn: &LcuConnection,
    settings: &Settings,
    tx: &crossbeam_channel::Sender<GuiMsg>,
    session_json: serde_json::Value,
    handled_actions: &mut HashMap<i64, ActionState>,
    shared_heroes: &Arc<Mutex<HashMap<i32, Hero>>>,
    local_cell_id: i64,
) {
    // 获取计时器信息
    let timer_obj = session_json.get("timer");
    let time_left_ms = timer_obj
        .and_then(|v| v.get("adjustedTimeLeftInPhase"))
        .and_then(|v| v.as_f64())
        .unwrap_or(99999.0);
    
    // 获取当前阶段 (BAN_PICK, PLANNING, BANNING, PICKING 等)
    // BAN_PICK 是 Ban Intent 阶段（所有人同时选择，不能锁定）
    // PLANNING 是 Pick Intent 阶段
    // BANNING/PICKING 是轮到自己操作的阶段（可以锁定）
    let timer_phase = timer_obj
        .and_then(|v| v.get("phase"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // 获取自己的位置
    let mut my_pos = String::new();
    // 收集队友的意向英雄（用于避免禁用队友想玩的英雄）
    let mut teammate_intents: HashSet<i32> = HashSet::new();

    if let Some(tm) = session_json.get("myTeam").and_then(|v| v.as_array()) {
        for m in tm {
            let cid = m.get("cellId").and_then(|x| x.as_i64()).unwrap_or(-2);
            if cid == local_cell_id {
                my_pos = m.get("assignedPosition")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_lowercase();
            } else {
                // 收集队友意向
                if let Some(intent) = m.get("championPickIntent").and_then(|v| v.as_i64()) {
                    if intent > 0 {
                        teammate_intents.insert(intent as i32);
                    }
                }
            }
        }
    }

    let now = Instant::now();

    // 遍历所有 actions
    if let Some(action_groups) = session_json.get("actions").and_then(|v| v.as_array()) {
        for group in action_groups {
            if let Some(list) = group.as_array() {
                for act in list {
                    let actor_cell_id = act.get("actorCellId").and_then(|v| v.as_i64()).unwrap_or(-9);
                    // 只处理自己的 action
                    if actor_cell_id != local_cell_id {
                        continue;
                    }

                    let action_id = act.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
                    let is_completed = act.get("completed").and_then(|v| v.as_bool()).unwrap_or(false);
                    let is_in_progress = act.get("isInProgress").and_then(|v| v.as_bool()).unwrap_or(false);
                    let type_str = act.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    let current_champ_id = act.get("championId").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

                    // 获取/创建该 action 的状态
                    let state = handled_actions.entry(action_id).or_default();

                    // 如果已完成，标记并跳过
                    if is_completed {
                        state.completed = true;
                        state.lock_scheduled_at = None; // 清除定时
                        continue;
                    }
                    // 如果我们已经处理完成过，跳过
                    if state.completed {
                        continue;
                    }

                    // 节流：防止短时间内重复发送高亮请求
                    let should_act = state.last_act_time
                        .map(|t| now.duration_since(t) > Duration::from_millis(500))
                        .unwrap_or(true);

                    // ========== BAN 阶段处理 ==========
                    if type_str == "ban" {
                        handle_ban_action(
                            client, conn, settings, tx, shared_heroes,
                            &my_pos, &teammate_intents,
                            action_id, is_in_progress, current_champ_id,
                            time_left_ms, timer_phase,
                            state, should_act, now,
                        ).await;
                    }
                    // ========== PICK 阶段处理 ==========
                    else if type_str == "pick" {
                        handle_pick_action(
                            client, conn, settings, tx, shared_heroes,
                            &my_pos,
                            action_id, is_in_progress, current_champ_id,
                            time_left_ms, timer_phase,
                            state, should_act, now,
                        ).await;
                    }
                }
            }
        }
    }
}

/// 处理 Ban 阶段的 action
/// 
/// 定时锁定逻辑：
/// 1. 只在非 BAN_PICK 阶段（Ban Intent）时才启动锁定定时器
/// 2. BAN_PICK 阶段只能高亮，不能锁定
async fn handle_ban_action(
    client: &Client,
    conn: &LcuConnection,
    settings: &Settings,
    tx: &crossbeam_channel::Sender<GuiMsg>,
    shared_heroes: &Arc<Mutex<HashMap<i32, Hero>>>,
    my_pos: &str,
    teammate_intents: &HashSet<i32>,
    action_id: i64,
    is_in_progress: bool,
    current_champ_id: i32,
    time_left_ms: f64,
    timer_phase: &str,
    state: &mut ActionState,
    should_act: bool,
    now: Instant,
) {
    // 如果未启用自动禁用，直接返回
    if !settings.sr_ban_enable {
        return;
    }

    // 获取预设的禁用目标英雄ID
    let mut preset_target = *settings.sr_bans.get(my_pos).unwrap_or(&0);
    
    // 如果预设目标是队友想玩的英雄，尝试从备选池中选择其他英雄
    if preset_target > 0 && teammate_intents.contains(&preset_target) {
        let alt_pool: Vec<i32> = settings.sr_bans
            .values()
            .cloned()
            .filter(|&id| id > 0 && !teammate_intents.contains(&id))
            .collect();
        if !alt_pool.is_empty() {
            preset_target = alt_pool[(action_id as usize) % alt_pool.len()];
            tx.send(GuiMsg::Log(format!(
                "禁用目标冲突，改为: {}",
                lookup_hero_name_by_id(shared_heroes, preset_target)
            ))).ok();
        } else {
            preset_target = 0;
        }
    }

    // 检查是否是意向阶段（PLANNING = 所有人同时选择意向）
    // 只有 PLANNING 阶段不启动定时器，BAN_PICK 是实际 Ban 阶段可以锁定
    let is_planning_phase = timer_phase == "PLANNING";

    // 当前轮到我操作 Ban
    if is_in_progress {
        //   日志：显示当前状态
        // if !state.hovered || state.lock_scheduled_at.is_none() {
        //     tx.send(GuiMsg::Log(format!(
        //         "[Ban调试] 阶段:{} 锁定已设:{} hovered:{} auto_ban_lock:{}",
        //         timer_phase,
        //         state.lock_scheduled_at.is_some(),
        //         state.hovered,
        //         settings.auto_ban_lock
        //     ))).ok();
        // }
        
        // 1. 首次进入 isInProgress 且非 PLANNING 阶段，设置锁定时间点
        if !is_planning_phase && state.lock_scheduled_at.is_none() && settings.auto_ban_lock {
            let wait_ms = time_left_ms - (settings.ban_time as f64 * 1000.0);
            let wait_duration = if wait_ms > 0.0 {
                Duration::from_millis(wait_ms as u64)
            } else {
                Duration::ZERO
            };
            
            state.lock_scheduled_at = Some(now + wait_duration);
            
            let scheduled_in_sec = wait_duration.as_secs_f64();
            tx.send(GuiMsg::Log(format!(
                "计划在 {:.1} 秒后锁定禁用 (设定剩余 {}s, 阶段: {})",       
                scheduled_in_sec, settings.ban_time, timer_phase
            ))).ok();
        }
        
        // 2. 尝试高亮预设目标（无论什么阶段都可以高亮）
        if current_champ_id == 0 && preset_target > 0 && should_act {
            let _ = lcu_patch_action(client, conn, action_id, preset_target, false).await;
            state.last_act_time = Some(now);
            if !state.hovered {
                tx.send(GuiMsg::Log(format!(
                    "准备禁用: {} (阶段: {})",
                    lookup_hero_name_by_id(shared_heroes, preset_target),
                    timer_phase
                ))).ok();
                state.hovered = true;
            }
        }
        
        // 3. 检查是否到达锁定时间点（不再检查 can_lock，与 Pick 一致）
        if let Some(lock_at) = state.lock_scheduled_at {
            if now >= lock_at {
                let lock_target = if current_champ_id > 0 { current_champ_id } else { preset_target };
                
                if lock_target > 0 {
                    tx.send(GuiMsg::Log(format!(
                        "执行锁定禁用: {} (阶段: {})",
                        lookup_hero_name_by_id(shared_heroes, lock_target),
                        timer_phase
                    ))).ok();
                    let _ = lcu_patch_action(client, conn, action_id, lock_target, true).await;
                    state.completed = true;
                    state.lock_scheduled_at = None;
                }
            }
        }
    } else {
        // 还没轮到我 Ban，清除定时并尝试预高亮
        state.lock_scheduled_at = None;
        
        if preset_target > 0 && current_champ_id != preset_target && should_act && !state.hovered {
            let _ = lcu_patch_action(client, conn, action_id, preset_target, false).await;
            state.last_act_time = Some(now);
            state.hovered = true;
        }
    }
}

/// 处理 Pick 阶段的 action
async fn handle_pick_action(
    client: &Client,
    conn: &LcuConnection,
    settings: &Settings,
    tx: &crossbeam_channel::Sender<GuiMsg>,
    shared_heroes: &Arc<Mutex<HashMap<i32, Hero>>>,
    my_pos: &str,
    action_id: i64,
    is_in_progress: bool,
    current_champ_id: i32,
    time_left_ms: f64,
    _timer_phase: &str, // 保留参数以保持接口一致性
    state: &mut ActionState,
    should_act: bool,
    now: Instant,
) {
    // 获取预设的选择目标英雄ID
    let preset_target = *settings.sr_picks.get(my_pos).unwrap_or(&0);

    // 当前轮到我操作 Pick
    if is_in_progress {
        // 1. 首次进入 isInProgress，设置锁定时间点
        if state.lock_scheduled_at.is_none() && settings.auto_lock {
            let wait_ms = time_left_ms - (settings.lock_time as f64 * 1000.0);
            let wait_duration = if wait_ms > 0.0 {
                Duration::from_millis(wait_ms as u64)
            } else {
                Duration::ZERO
            };
            
            state.lock_scheduled_at = Some(now + wait_duration);
            
            let scheduled_in_sec = wait_duration.as_secs_f64();
            tx.send(GuiMsg::Log(format!(
                "计划在 {:.1} 秒后锁定选择 (设定剩余 {}s 时锁定)",
                scheduled_in_sec, settings.lock_time
            ))).ok();
        }
        
        // 2. 尝试高亮预设目标（如果当前没有高亮且启用了预选）
        if current_champ_id == 0 && preset_target > 0 && should_act && settings.sr_enable {
            let _ = lcu_patch_action(client, conn, action_id, preset_target, false).await;
            state.last_act_time = Some(now);
            if !state.hovered {
                tx.send(GuiMsg::Log(format!(
                    "正在预选: {}",
                    lookup_hero_name_by_id(shared_heroes, preset_target)
                ))).ok();
                state.hovered = true;
            }
        }
        
        // 3. 检查是否到达锁定时间点
        if let Some(lock_at) = state.lock_scheduled_at {
            if now >= lock_at {
                // 锁定当前高亮的英雄
                let lock_target = if current_champ_id > 0 { current_champ_id } else { preset_target };
                
                if lock_target > 0 {
                    tx.send(GuiMsg::Log(format!(
                        "执行锁定选择: {}",
                        lookup_hero_name_by_id(shared_heroes, lock_target)
                    ))).ok();
                    let _ = lcu_patch_action(client, conn, action_id, lock_target, true).await;
                    state.completed = true;
                    state.lock_scheduled_at = None;
                }
            }
        }
    } else {
        // 还没轮到我 Pick，清除定时并展示意向
        state.lock_scheduled_at = None;
        
        if settings.sr_enable && preset_target > 0 && current_champ_id != preset_target && should_act && !state.hovered {
            let _ = lcu_patch_action(client, conn, action_id, preset_target, false).await;
            state.last_act_time = Some(now);
            tx.send(GuiMsg::Log(format!(
                "展示意向: {}",
                lookup_hero_name_by_id(shared_heroes, preset_target)
            ))).ok();
            state.hovered = true;
        }
    }
}

/// 发送 PATCH 请求修改 action（选择/禁用英雄）
async fn lcu_patch_action(
    client: &Client,
    conn: &LcuConnection,
    action_id: i64,
    champ_id: i32,
    completed: bool,
) -> anyhow::Result<()> {
    let url = format!("{}/lol-champ-select/v1/session/actions/{}", conn.url, action_id);
    let body = serde_json::json!({
        "championId": champ_id,
        "completed": completed
    });
    let _ = client
        .patch(&url)
        .header("Authorization", &conn.auth_header)
        .json(&body)
        .send()
        .await;
    Ok(())
}

/// 大乱斗板凳席处理
async fn handle_aram_bench(
    client: &Client,
    conn: &LcuConnection,
    settings: &Settings,
    tx: &crossbeam_channel::Sender<GuiMsg>,
    session: serde_json::Value,
    last_bench_ids: &mut Vec<i32>,
    local_cell_id: i64,
    shared_heroes: &Arc<Mutex<HashMap<i32, Hero>>>,
) {
    if let Some(bench) = session.get("benchChampions").and_then(|v| v.as_array()) {
        let current_bench: Vec<i32> = bench
            .iter()
            .filter_map(|x| x.get("championId").and_then(|i| i.as_i64()).map(|i| i as i32))
            .collect();

        // 检查板凳席是否有变化，通知 UI 更新
        let mut sorted = current_bench.clone();
        sorted.sort();
        let mut last = last_bench_ids.clone();
        last.sort();
        if sorted != last {
            *last_bench_ids = current_bench.clone();
            tx.send(GuiMsg::BenchUpdate(current_bench.clone())).ok();
        }

        // 秒抢逻辑
        if settings.aram_snipe {
            let mut my_hero = 0;
            if let Some(tm) = session.get("myTeam").and_then(|x| x.as_array()) {
                for m in tm {
                    if m.get("cellId").and_then(|x| x.as_i64()).unwrap_or(-2) == local_cell_id {
                        my_hero = m.get("championId").and_then(|x| x.as_i64()).unwrap_or(0) as i32;
                        break;
                    }
                }
            }

            // 遍历秒抢列表，按优先级尝试交换
            for &tid in &settings.snipe_list {
                // 如果已经是目标英雄，停止
                if my_hero == tid {
                    break;
                }
                // 如果目标在板凳席上，执行交换
                if current_bench.contains(&tid) {
                    let _ = lcu_request(
                        client,
                        conn,
                        "POST",
                        &format!("/lol-champ-select/v1/session/bench/swap/{}", tid),
                        None,
                    ).await;
                    // 显示英雄中文名
                    tx.send(GuiMsg::Log(format!(
                        "秒抢: {}",
                        lookup_hero_name_by_id(shared_heroes, tid)
                    ))).ok();
                    break;
                }
            }
        }
    }
}