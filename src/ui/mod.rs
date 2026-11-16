use chrono::Local;
use eframe::{egui, App};
use egui::{Color32, FontData, FontDefinitions, FontFamily, Vec2};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::models::{BackendMsg, GuiMsg, Hero, Settings, TIER_MAP};
use crate::utils::{load_settings, IMG_DIR};
mod aram;
mod micro;
mod normal;
mod sr;
mod widgets;
mod window;

pub use widgets::*;
pub use window::clamp_to_screen;

#[cfg(test)]
mod tests;


pub struct HexApp {
    log_lines: Vec<String>,
    connected: bool,
    status_text: String,
    tx_to_backend: crossbeam_channel::Sender<BackendMsg>,
    rx_from_backend: crossbeam_channel::Receiver<GuiMsg>,
    settings: Settings,
    heroes: Arc<Mutex<HashMap<i32, Hero>>>,
    bench_ids: Vec<i32>,
    /// 当前是否大乱斗（后端 benchEnabled）；微缩模式据此显隐板凳席
    is_aram: bool,
    search_input: String,
    search_result: Vec<Hero>,
    /// 抢人搜索下拉是否展开
    snipe_open: bool,
    /// 下拉列表中键盘高亮的行
    snipe_highlight: usize,
    /// 单例英雄选择浮窗目标：(位置 key, 是否禁用列)
    sr_picker: Option<(String, bool)>,
    picker_query: String,
    picker_results: Vec<Hero>,
    picker_highlight: usize,
    picker_anchor: egui::Rect,
    /// 上次发送的自适应窗口高度，避免重复下发命令
    last_window_h: f32,
    /// 上次发送的自适应窗口宽度，避免重复下发命令
    last_window_w: f32,
    /// 统一卡片内容宽度：所有卡片对齐到最宽卡片，只增不减以收敛
    content_w: f32,
    /// 上次记录的窗口位置，用于检测移动并持久化
    last_window_pos: Option<egui::Pos2>,
    /// 是否已在启动时恢复过窗口位置
    restored_pos: bool,
    /// 启动恢复位置剩余下发帧数（幂等重复，确保命令生效）
    restore_frames: u8,
    /// 窗口位置最近一次变化的时刻；稳定 300ms 后防抖写盘
    pos_dirty_since: Option<std::time::Instant>,
    /// 手动拖拽抓取点（指针相对窗口左上角的偏移）；None 表示未拖拽
    drag_offset: Option<egui::Vec2>,
    /// 拖拽期间自己上次下发的窗口左上角（整像素）。outer_rect 比实际位置滞后一帧，
    /// 且系统落位为整像素，二者都不能作换算基准，否则不跟手 / 静止时像素级来回抖。
    drag_base: Option<egui::Pos2>,
    /// 上次换算所用的指针本地坐标；egui 本地坐标不随窗口移动重基，
    /// 无新指针事件的帧重复换算会把自身位移再计一次（原地横跳），据此跳过。
    drag_last_lp: Option<egui::Pos2>,
    image_cache: HashMap<String, egui::TextureHandle>,
    /// 是否在准备检查中
    in_ready_check: bool,
    /// 当前游戏阶段
    current_phase: String,
}
impl HexApp {
    pub fn new(
        ctx: &egui::Context,
        tx: crossbeam_channel::Sender<BackendMsg>,
        rx: crossbeam_channel::Receiver<GuiMsg>,
        heroes: Arc<Mutex<HashMap<i32, Hero>>>,
    ) -> Self {
        let mut fonts = FontDefinitions::default();
        let font_name = "sys_font".to_owned();
        let mut loaded = false;
        if let Ok(data) = fs::read("C:\\Windows\\Fonts\\msyh.ttc") {
            fonts
                .font_data
                .insert(font_name.clone(), FontData::from_owned(data));
            loaded = true;
        } else if let Ok(data) = fs::read("C:\\Windows\\Fonts\\simhei.ttf") {
            fonts
                .font_data
                .insert(font_name.clone(), FontData::from_owned(data));
            loaded = true;
        }
        if loaded {
            fonts
                .families
                .get_mut(&FontFamily::Proportional)
                .unwrap()
                .insert(0, font_name.clone());
            fonts
                .families
                .get_mut(&FontFamily::Monospace)
                .unwrap()
                .insert(0, font_name);
            ctx.set_fonts(fonts);
        }
        ctx.set_theme(egui::Theme::Dark);
        configure_visuals(ctx);
        let settings = load_settings();
        Self {
            log_lines: vec!["初始化...".into()],
            connected: false,
            status_text: "未连接".into(),
            tx_to_backend: tx,
            rx_from_backend: rx,
            settings,
            heroes,
            bench_ids: vec![],
            is_aram: false,
            search_input: String::new(),
            search_result: vec![],
            snipe_open: false,
            snipe_highlight: 0,
            sr_picker: None,
            picker_query: String::new(),
            picker_results: vec![],
            picker_highlight: 0,
            picker_anchor: egui::Rect::NOTHING,
            last_window_h: 0.0,
            last_window_w: 0.0,
            content_w: 300.0,
            last_window_pos: None,
            restored_pos: false,
            restore_frames: 0,
            pos_dirty_since: None,
            drag_offset: None,
            drag_base: None,
            drag_last_lp: None,
            image_cache: HashMap::new(),
            in_ready_check: false,
            current_phase: String::new(),
        }
    }

    fn trigger_save(&self) {
        let _ = self
            .tx_to_backend
            .send(BackendMsg::SaveSettings(self.settings.clone()));
    }

    /// 根据当前阶段更新状态文本
    fn update_status_from_phase(&mut self) {
        self.status_text = match self.current_phase.as_str() {
            "Lobby" => "● 房间中".into(),
            "Matchmaking" => "● 匹配中".into(),
            "ReadyCheck" => "准备检查中".into(),
            "ChampSelect" => "● 选人中".into(),
            "InProgress" | "GameStart" => "● 游戏中".into(),
            "WaitingForStats" | "PreEndOfGame" | "EndOfGame" => "● 结算中".into(),
            _ => "● 已连接".into(),
        };
    }

    fn get_image(&mut self, ctx: &egui::Context, image_name: &str) -> Option<egui::TextureHandle> {
        if let Some(handle) = self.image_cache.get(image_name) {
            return Some(handle.clone());
        }
        // Load image
        let path = format!("{}/{}", IMG_DIR, image_name);
        if Path::new(&path).exists() {
            if let Ok(data) = fs::read(&path) {
                let reader = image::load_from_memory(&data).ok()?;
                let size = [reader.width() as _, reader.height() as _];
                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    size,
                    reader.to_rgba8().as_flat_samples().as_slice(),
                );
                let handle = ctx.load_texture(image_name, color_image, Default::default());
                self.image_cache
                    .insert(image_name.to_string(), handle.clone());
                return Some(handle);
            }
        } else {
            let url = format!(
                "https://ddragon.leagueoflegends.com/cdn/14.23.1/img/champion/{}",
                image_name
            );
            // 简单卡UI也不管了，反正很少
            if let Ok(resp) = reqwest::blocking::get(url) {
                if let Ok(bytes) = resp.bytes() {
                    let _ = fs::create_dir_all(IMG_DIR);
                    let _ = fs::write(&path, &bytes);
                    if let Ok(reader) = image::load_from_memory(&bytes) {
                        let size = [reader.width() as _, reader.height() as _];
                        let color_image = egui::ColorImage::from_rgba_unmultiplied(
                            size,
                            reader.to_rgba8().as_flat_samples().as_slice(),
                        );
                        let handle = ctx.load_texture(image_name, color_image, Default::default());
                        self.image_cache
                            .insert(image_name.to_string(), handle.clone());
                        return Some(handle);
                    }
                }
            }
        }
        None
    }

    fn update_ui(&mut self, ctx: &egui::Context) {
        self.sync_window_position(ctx);
        while let Ok(msg) = self.rx_from_backend.try_recv() {
            match msg {
                GuiMsg::Log(s) => {
                    self.log_lines
                        .push(format!("[{}] {}", Local::now().format("%H:%M:%S"), s));
                    if self.log_lines.len() > 50 {
                        self.log_lines.remove(0);
                    }
                }
                GuiMsg::Status(c) => {
                    self.connected = c;
                    if !c {
                        self.status_text = "未连接".into();
                        self.current_phase = String::new();
                        self.in_ready_check = false;
                        self.is_aram = false;
                    }
                }
                GuiMsg::BenchUpdate(ids) => self.bench_ids = ids,
                GuiMsg::AramMode(b) => self.is_aram = b,
                GuiMsg::LoadedData(s) => {
                    self.status_text = s;
                }
                GuiMsg::ReadyCheckState(in_check) => {
                    self.in_ready_check = in_check;
                    if in_check {
                        self.status_text = "准备检查中".into();
                    } else if self.connected {
                        self.update_status_from_phase();
                    }
                }
                GuiMsg::PhaseUpdate(phase) => {
                    if phase != "ChampSelect" {
                        self.is_aram = false;
                    }
                    self.current_phase = phase;
                    if self.connected && !self.in_ready_check {
                        self.update_status_from_phase();
                    }
                }
            }
        }

        // Custom Window Frame: Deep Blue Fill, Gold Border, 8px Rounding
        let outer_margin = 6.0;
        let panel_frame = egui::Frame::window(&ctx.style())
            .fill(Color32::from_rgb(1, 10, 19)) // Deep Blue
            .inner_margin(outer_margin)
            .rounding(egui::Rounding::same(0.0));
        // .stroke(egui::Stroke::new(1.5, Color32::from_rgb(120, 90, 40))); // Hextech Gold Border

        egui::CentralPanel::default()
            .frame(panel_frame)
            .show(ctx, |ui| {
                let app_rect = ui.max_rect();
                let title_bar_height = 32.0;
                let title_bar_rect = egui::Rect::from_min_size(
                    app_rect.min,
                    egui::vec2(app_rect.width(), title_bar_height),
                );
                let content_rect = egui::Rect::from_min_max(
                    egui::Pos2::new(app_rect.min.x, app_rect.min.y + title_bar_height),
                    app_rect.max,
                );


                // Title Bar Background & Drag
                let painter = ui.painter();
                let title_bg = Color32::from_rgb(4, 10, 20);
                painter.rect_filled(title_bar_rect, egui::Rounding::same(4.0), title_bg);

                let title_bar_response = ui.interact(
                    title_bar_rect,
                    egui::Id::new("title_bar"),
                    egui::Sense::click_and_drag(),
                );
                // 手动拖拽：不进入 Windows 原生模态循环（屏蔽边缘分屏）。
                // 基准用自己上次下发的整像素位置：outer_rect 滞后一帧会少移一个上帧位移
                // （不跟手），浮点基准与系统整像素落位的小数差会反馈成静止时的像素级抖动。
                // 下发整像素命令保证基准与实际落位严格一致，小数增量累积超过半像素才移动。
                let pointer_down = ctx.input(|i| i.pointer.any_down());
                if title_bar_response.drag_started() {
                    if let (Some(lp), Some(outer)) = (
                        ctx.input(|i| i.pointer.latest_pos()),
                        ctx.input(|i| i.viewport().outer_rect),
                    ) {
                        self.drag_offset = Some(lp.to_vec2());
                        self.drag_base = Some(outer.min.round());
                        self.drag_last_lp = Some(lp);
                    }
                }
                if let Some(off) = self.drag_offset {
                    if !pointer_down {
                        self.drag_offset = None;
                        self.drag_base = None;
                        self.drag_last_lp = None;
                    } else if let (Some(base), Some(lp)) =
                        (self.drag_base, ctx.input(|i| i.pointer.latest_pos()))
                    {
                        // 只在有新指针事件的帧换算：本地坐标不随窗口移动重基，
                        // 静止帧重复换算会把自身位移逐帧累加（ runaway 横跳）。
                        if self.drag_last_lp != Some(lp) {
                            self.drag_last_lp = Some(lp);
                            let desired = base + lp.to_vec2() - off;
                            // 1px 死区：过滤静止时高回报率鼠标的微抖，避免像素级来回摆
                            if (desired.x - base.x).abs() > 1.0
                                || (desired.y - base.y).abs() > 1.0
                            {
                                let target = desired.round();
                                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(
                                    target,
                                ));
                                self.drag_base = Some(target);
                                self.last_window_pos = Some(target);
                                self.settings.window_pos = Some([target.x, target.y]);
                                self.pos_dirty_since = Some(std::time::Instant::now());
                            }
                        }
                        ctx.request_repaint();
                        ctx.request_repaint_after(Duration::from_millis(300));
                    }
                }
                // 双击标题栏切换极简微缩模式（本地持久化）
                if title_bar_response.double_clicked() {
                    self.settings.micro_mode = !self.settings.micro_mode;
                    self.trigger_save();
                }
                // Title Bar Content
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(title_bar_rect), |ui| {
                    if self.settings.micro_mode {
                        self.render_micro_title(ui, ctx, title_bar_rect);
                        return;
                    }
                    self.render_normal_title(ui, ctx, title_bar_rect);
                });

                // Main Content
                let content_size = ui.allocate_new_ui(egui::UiBuilder::new().max_rect(content_rect), |ui| {
                    ui.add_space(4.0);
                    if self.settings.micro_mode {
                        return self.render_micro_content(ui, ctx);
                    }

                    self.render_normal_content(ui, ctx)
                })
                .inner;

                // 卡片统一宽度：向最宽卡片对齐。content_size.x 为最宽卡片外框，
                // 减去卡片内边距即所需内容宽；只增不减，保证收敛不抖动。
                let needed_w = content_size.x - 12.0;
                if needed_w > self.content_w {
                    self.content_w = needed_w;
                }

                // 四、窗口尺寸自适应内容。高度 = 标题栏 + 内容(含顶距) + 上下外边距；
                // 宽度 = 内容自然宽 + 左右外边距。外边距必须计入，否则窗口比内容矮/窄
                // 几个像素就会溢出。无 ScrollArea，故不会出现滚动条。
                // 上限用显示器尺寸（稳定），不能用 screen_rect——后者等于窗口自身尺寸，
                // 以其做 clamp 会每帧把窗口缩小一点，最终塌缩成一团。
                let monitor = ctx.input(|i| i.viewport().monitor_size);
                let monitor_h = monitor.map(|s| s.y).unwrap_or(10_000.0);
                let monitor_w = monitor.map(|s| s.x).unwrap_or(10_000.0);
                // 微缩模式内容极短，下限必须放低，否则板凳席隐藏时窗口仍占一大块空白
                let (min_h, min_w) = if self.settings.micro_mode {
                    (60.0, 120.0)
                } else {
                    (220.0, 240.0)
                };
                let target_h = (title_bar_height + content_size.y + 2.0 * outer_margin)
                    .min(monitor_h - 60.0)
                    .max(min_h);
                let target_w = (content_size.x + 2.0 * outer_margin)
                    .min(monitor_w - 60.0)
                    .max(min_w);
                if (target_h - self.last_window_h).abs() > 0.5
                    || (target_w - self.last_window_w).abs() > 0.5
                {
                    self.last_window_h = target_h;
                    self.last_window_w = target_w;
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        target_w, target_h,
                    )));
                    // 尺寸变化（微缩↔完整切换）后按新尺寸重新夹回屏幕：
                    // 小窗贴边时恢复大窗，若不夹回则右侧/下侧会越界。
                    if let Some(p) = self.last_window_pos {
                        let mon = monitor.unwrap_or(egui::vec2(1920.0, 1080.0));
                        let cx = clamp_to_screen(p.x, target_w, mon.x, 24.0);
                        let cy = clamp_to_screen(p.y, target_h, mon.y, 24.0);
                        if (cx - p.x).abs() > 0.5 || (cy - p.y).abs() > 0.5 {
                            let np = egui::pos2(cx, cy);
                            self.last_window_pos = Some(np);
                            self.settings.window_pos = Some([cx, cy]);
                            self.restore_frames = 3;
                            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(np));
                            self.trigger_save();
                        }
                    }
                }
            });

        self.show_sr_picker(ctx);

        ctx.request_repaint_after(Duration::from_secs(2));
    }
}


impl App for HexApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_ui(ctx);
    }
}
