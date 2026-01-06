use chrono::Local;
use eframe::{egui, App};
use egui::{Color32, FontData, FontDefinitions, FontFamily, Vec2};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::models::{BackendMsg, GuiMsg, Hero, Settings, TIER_MAP};
use crate::utils::{load_settings, lookup_hero_id, lookup_hero_image_by_text, lookup_hero_name_by_text, IMG_DIR};

pub fn configure_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();

    // 1. Window & Panel Basics (Transparent for custom shape)
    visuals.window_fill = Color32::TRANSPARENT;
    visuals.panel_fill = Color32::from_rgb(1, 10, 19); // Deep Hextech Blue
    visuals.window_rounding = egui::Rounding::same(8.0);

    // 2. Backgrounds for different layers
    visuals.faint_bg_color = Color32::from_rgb(5, 15, 25);
    visuals.extreme_bg_color = Color32::from_rgb(1, 10, 19); // Input/TextEdit fixed to Deep Blue
    visuals.code_bg_color = Color32::from_rgb(1, 10, 19);

    // 3. Widget States
    // Non-interactive (Labels, containers)
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(10, 20, 30);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, Color32::from_gray(200));

    // Inactive (Buttons, Checkboxes)
    // IMPORTANT: bg_fill here controls the checkbox background and unhovered button background!
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(20, 30, 40);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(20, 30, 40);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, Color32::from_rgb(220, 220, 220)); // Bright Silver Text
    visuals.widgets.inactive.rounding = egui::Rounding::same(4.0);

    // Hovered
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(0, 90, 130); // Hextech Interact Blue
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, Color32::WHITE); // Force White Text
    visuals.widgets.hovered.rounding = egui::Rounding::same(4.0);

    // Active (Click / Drag / Checked)
    visuals.widgets.active.bg_fill = Color32::from_rgb(200, 170, 110); // Hextech Gold
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.rounding = egui::Rounding::same(4.0);

    // Open (Dropdowns)
    visuals.widgets.open.bg_fill = Color32::from_rgb(10, 25, 40);
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, Color32::WHITE);

    // Selection (Text select)
    visuals.selection.bg_fill = Color32::from_rgb(0, 90, 130);
    visuals.selection.stroke = egui::Stroke::new(1.0, Color32::WHITE);

    ctx.set_visuals(visuals);
}

pub struct HexApp {
    log_lines: Vec<String>,
    connected: bool,
    status_text: String,
    tx_to_backend: crossbeam_channel::Sender<BackendMsg>,
    rx_from_backend: crossbeam_channel::Receiver<GuiMsg>,
    settings: Settings,
    heroes: Arc<Mutex<HashMap<i32, Hero>>>,
    bench_ids: Vec<i32>,
    search_input: String,
    search_result: Vec<Hero>,
    sr_pick_text: HashMap<String, String>,
    sr_ban_text: HashMap<String, String>,
    image_cache: HashMap<String, egui::TextureHandle>,
}

impl HexApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
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
            _cc.egui_ctx.set_fonts(fonts);
        }
        _cc.egui_ctx.set_theme(egui::Theme::Dark);
        configure_visuals(&_cc.egui_ctx);
        Self {
            log_lines: vec!["初始化...".into()],
            connected: false,
            status_text: "未连接".into(),
            tx_to_backend: tx,
            rx_from_backend: rx,
            settings: load_settings(),
            heroes,
            bench_ids: vec![],
            search_input: String::new(),
            search_result: vec![],
            sr_pick_text: HashMap::new(),
            sr_ban_text: HashMap::new(),
            image_cache: HashMap::new(),
        }
    }

    fn sync_ui_names(&mut self) {
        let map = self.heroes.lock().unwrap();
        if map.is_empty() {
            return;
        }
        let pos_keys = vec!["top", "jungle", "middle", "bottom", "utility"];
        for k in pos_keys {
            // Always update to reflect loaded settings/names
            let pick_name = self
                .settings
                .sr_picks
                .get(k)
                .and_then(|id| map.get(id))
                .map(|h| h.name.clone())
                .unwrap_or_default();
            self.sr_pick_text.insert(k.to_string(), pick_name);

            let ban_name = self
                .settings
                .sr_bans
                .get(k)
                .and_then(|id| map.get(id))
                .map(|h| h.name.clone())
                .unwrap_or_default();
            self.sr_ban_text.insert(k.to_string(), ban_name);
        }
    }

    fn trigger_save(&self) {
        let _ = self
            .tx_to_backend
            .send(BackendMsg::SaveSettings(self.settings.clone()));
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
}

impl App for HexApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
                    self.status_text = if c {
                        "● 已连接".into()
                    } else {
                        "○ 未连接".into()
                    };
                }
                GuiMsg::BenchUpdate(ids) => self.bench_ids = ids,
                GuiMsg::LoadedData(s) => {
                    self.status_text = s;
                    self.sync_ui_names();
                }
            }
        }

        // Custom Window Frame: Deep Blue Fill, Gold Border, 8px Rounding
        let panel_frame = egui::Frame::window(&ctx.style())
            .fill(Color32::from_rgb(1, 10, 19)) // Deep Blue
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
                if title_bar_response.dragged() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                // Title Bar Content
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(title_bar_rect), |ui| {
                    ui.style_mut().interaction.selectable_labels = false; // Disable text selection
                    ui.horizontal_centered(|ui| {
                        ui.add_space(8.0);
                        // ui.heading("海克斯 • 连接器");
                        ui.label(
                            egui::RichText::new("海克斯 • 连接器")
                                .heading()
                                .strong() // 加重显示
                                .color(egui::Color32::from_rgb(205, 190, 145)), // .background_color(egui::Color32::from_rgba_unmultiplied(100, 100, 100, 50)) // 可选：加个半透明深色底衬托浅色字
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(8.0);

                            // Close Button
                            let btn_close = egui::Button::new(
                                egui::RichText::new(" × ")
                                    .color(egui::Color32::WHITE) // 红色背景配白色文字
                                    .strong(),
                            )
                            .fill(egui::Color32::from_rgb(200, 50, 50)) // 基础红色
                            .rounding(2.0); // 轻微圆角
                            if ui.add(btn_close).clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }

                            // Minimize Button
                            let btn_min = egui::Button::new(
                                egui::RichText::new(" - ").color(Color32::from_gray(200)),
                            )
                            .frame(false)
                            .fill(Color32::TRANSPARENT);
                            if ui.add(btn_min).clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                            }

                            ui.add_space(8.0);
                            //↻重连
                            if ui
                                .add(egui::Button::new("↻").fill(Color32::from_rgb(20, 30, 40)))
                                .clicked()
                            {
                                let _ = self.tx_to_backend.send(BackendMsg::ForceReconnect);
                            }
                            
                            let status_color = if self.connected {
                                Color32::from_rgb(10, 203, 230) // Hextech Blue
                            } else {
                                Color32::RED
                            };
                        
                            ui.label(egui::RichText::new(&self.status_text).color(status_color));
                        });
                    });
                });

                // Main Content
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(content_rect), |ui| {
                    ui.add_space(4.0);

                    // ui.separator();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let frame_style = egui::Frame::none()
                            .fill(Color32::from_rgb(10, 25, 40)) // Slightly lighter blue for cards
                            .inner_margin(8.0)
                            .rounding(6.0)
                            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(120, 90, 40)));

                        frame_style.show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                egui::RichText::new("大乱斗板凳席")
                                    .color(Color32::from_rgb(200, 170, 110))
                                    .strong(),
                            );
                            ui.horizontal_wrapped(|ui| {
                                if self.bench_ids.is_empty() {
                                    ui.colored_label(Color32::GRAY, "等待队列中...");
                                } else {
                                    let bench_data: Vec<Hero> = {
                                        let map = self.heroes.lock().unwrap();
                                        self.bench_ids
                                            .iter()
                                            .filter_map(|id| map.get(id).cloned())
                                            .collect()
                                    };
                                    for h in bench_data {
                                        let img = egui::Image::new(
                                            &self.get_image(ctx, &h.image_name).unwrap(),
                                        )
                                        .fit_to_exact_size(Vec2::splat(32.0));
                                        if ui.add(egui::Button::image(img)).clicked() {
                                            let _ = self
                                                .tx_to_backend
                                                .send(BackendMsg::SwapChamp(h.id));
                                        }
                                    }
                                }
                            });
                        });
                        ui.add_space(8.0);

                        frame_style.show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                egui::RichText::new("全局功能")
                                    .color(Color32::from_rgb(200, 170, 110))
                                    .strong(),
                            );
                            ui.horizontal(|ui| {
                                if ui
                                    .checkbox(&mut self.settings.auto_accept, "自动接受")
                                    .changed()
                                {
                                    self.trigger_save();
                                }
                                if ui
                                    .checkbox(&mut self.settings.auto_honor, "自动点赞")
                                    .changed()
                                {
                                    self.trigger_save();
                                }
                            });
                            ui.horizontal(|ui| {
                                if ui
                                    .checkbox(&mut self.settings.play_again, "自动回房间")
                                    .changed()
                                {
                                    self.trigger_save();
                                }
                                if ui
                                    .checkbox(&mut self.settings.auto_queue, "自动匹配")
                                    .changed()
                                {
                                    self.trigger_save();
                                }
                                if ui.add(
                                    egui::DragValue::new(&mut self.settings.queue_delay)
                                        .range(1..=60)
                                        .suffix("s"),
                                ).changed() {
                                    self.trigger_save();
                                }
                            });
                            ui.horizontal(|ui| {
                                if ui
                                    .checkbox(&mut self.settings.spoof_rank, "伪装段位")
                                    .changed()
                                {
                                    self.trigger_save();
                                    let _ = self.tx_to_backend.send(BackendMsg::UpdateRank);
                                }
                                egui::ComboBox::from_id_salt("tier")
                                    .selected_text(
                                        TIER_MAP
                                            .iter()
                                            .find(|(k, _)| *k == self.settings.spoof_tier)
                                            .map(|(_, v)| *v)
                                            .unwrap_or(&self.settings.spoof_tier),
                                    )
                                    .show_ui(ui, |ui| {
                                        for (k, v) in TIER_MAP {
                                            if ui
                                                .selectable_value(
                                                    &mut self.settings.spoof_tier,
                                                    k.to_string(),
                                                    v,
                                                )
                                                .clicked()
                                            {
                                                self.trigger_save();
                                                let _ =
                                                    self.tx_to_backend.send(BackendMsg::UpdateRank);
                                            }
                                        }
                                    });
                                egui::ComboBox::from_id_salt("div")
                                    .selected_text(&self.settings.spoof_div)
                                    .show_ui(ui, |ui| {
                                        for d in ["IV", "III", "II", "I"] {
                                            if ui
                                                .selectable_value(
                                                    &mut self.settings.spoof_div,
                                                    d.to_string(),
                                                    d,
                                                )
                                                .clicked()
                                            {
                                                self.trigger_save();
                                                let _ =
                                                    self.tx_to_backend.send(BackendMsg::UpdateRank);
                                            }
                                        }
                                    });
                            });
                        });
                        ui.add_space(8.0);

                        frame_style.show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                egui::RichText::new("大乱斗抢人")
                                    .color(Color32::from_rgb(200, 170, 110))
                                    .strong(),
                            );
                            ui.horizontal(|ui| {
                                if ui.text_edit_singleline(&mut self.search_input).changed() {
                                    let map = self.heroes.lock().unwrap();
                                    let term = self.search_input.to_lowercase();
                                    self.search_result = map
                                        .values()
                                        .filter(|h| h.alias.contains(&term))
                                        .take(8)
                                        .cloned()
                                        .collect();
                                }
                            });
                            if !self.search_result.is_empty() && !self.search_input.is_empty() {
                                ui.horizontal_wrapped(|ui| {
                                    let mut clear = false;
                                    for h in &self.search_result {
                                        if ui.button(format!("+{}", h.name)).clicked() {
                                            if !self.settings.snipe_list.contains(&h.id) {
                                                self.settings.snipe_list.push(h.id);
                                                self.trigger_save();
                                            }
                                            clear = true;
                                        }
                                    }
                                    if clear {
                                        self.search_input.clear();
                                        self.search_result.clear();
                                    }
                                });
                            }
                            if ui
                                .checkbox(&mut self.settings.aram_snipe, "启用秒选")
                                .changed()
                            {
                                self.trigger_save();
                            }
                            let map = self.heroes.lock().unwrap();
                            let mut rm = None;
                            for (i, &id) in self.settings.snipe_list.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(map.get(&id).map(|h| h.name.as_str()).unwrap_or("?"));
                                    if ui.button("x").clicked() {
                                        rm = Some(i);
                                    }
                                });
                            }
                            if let Some(i) = rm {
                                self.settings.snipe_list.remove(i);
                                self.trigger_save();
                            }
                        });
                        ui.add_space(8.0);

                        frame_style.show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                egui::RichText::new("峡谷/排位")
                                    .color(Color32::from_rgb(200, 170, 110))
                                    .strong(),
                            );
                            ui.horizontal(|ui| {
                                if ui.checkbox(&mut self.settings.sr_enable, "预选").changed() {
                                    self.trigger_save();
                                }
                                if ui.checkbox(&mut self.settings.auto_lock, "锁定").changed() {
                                    self.trigger_save();
                                }
                                ui.label("倒计时:");
                                if ui.add(
                                    egui::DragValue::new(&mut self.settings.lock_time)
                                        .range(1..=30)
                                        .suffix("s"),
                                ).changed() {
                                    self.trigger_save();
                                }
                            });
                            ui.horizontal(|ui| {
                                if ui
                                    .checkbox(&mut self.settings.sr_ban_enable, "禁用")
                                    .changed()
                                {
                                    self.trigger_save();
                                }
                                if ui.checkbox(&mut self.settings.auto_ban_lock, "锁定").changed() {
                                    self.trigger_save();
                                }
                                ui.label("倒计时:");
                                if ui.add(
                                    egui::DragValue::new(&mut self.settings.ban_time)
                                        .range(1..=30)
                                        .suffix("s"),
                                ).changed() {
                                    self.trigger_save();
                                }
                            });
                            egui::Grid::new("sr_grid").striped(true).show(ui, |ui| {
                                ui.label("位置");
                                ui.label("预选");
                                ui.label("");
                                ui.label("禁用");
                                ui.label("");
                                ui.end_row();
                                let pos = vec![
                                    ("top", "上"),
                                    ("jungle", "野"),
                                    ("middle", "中"),
                                    ("bottom", "下"),
                                    ("utility", "辅"),
                                ];
                                for (k, l) in pos {
                                    ui.label(l);

                                    // 修复部分：不调用 trigger_save，直接发送消息
                                    // Pick Logic
                                    // 声明变量但不初始化，稍后在作用域中赋值，避免未使用赋值警告
                                    let pick_text;
                                    {
                                        let p_t = self.sr_pick_text.entry(k.to_string()).or_default();
                                        // 使用 add_sized 强制设置宽度，解决 Grid 布局中 desired_width 不生效的问题
                                        if ui
                                            .add_sized([70.0, 20.0], egui::TextEdit::singleline(p_t))
                                            .changed()
                                        {
                                            let id = lookup_hero_id(&self.heroes, p_t);
                                            self.settings.sr_picks.insert(k.to_string(), id);
                                            let _ = self
                                                .tx_to_backend
                                                .send(BackendMsg::SaveSettings(self.settings.clone()));
                                        }
                                        pick_text = p_t.clone();
                                    }

                                    let pick_img_name = lookup_hero_image_by_text(&self.heroes, &pick_text);
                                    if !pick_img_name.is_empty() {
                                        if let Some(texture) = self.get_image(ctx, &pick_img_name) {
                                            ui.image((texture.id(), Vec2::splat(20.0)));
                                        } else {
                                            // 图片加载失败或正在下载时，显示占位或保持空白
                                            ui.label("..."); 
                                        }
                                    } else {
                                        // 未找到英雄时显示文本（保留反馈）
                                        ui.colored_label(
                                            Color32::from_rgb(100, 200, 100),
                                            lookup_hero_name_by_text(&self.heroes, &pick_text),
                                        );
                                    }

                                    // Ban Logic
                                    let ban_text;
                                    {
                                        let b_t = self.sr_ban_text.entry(k.to_string()).or_default();
                                        if ui
                                            .add_sized([70.0, 20.0], egui::TextEdit::singleline(b_t))
                                            .changed()
                                        {
                                            let id = lookup_hero_id(&self.heroes, b_t);
                                            self.settings.sr_bans.insert(k.to_string(), id);
                                            let _ = self
                                                .tx_to_backend
                                                .send(BackendMsg::SaveSettings(self.settings.clone()));
                                        }
                                        ban_text = b_t.clone();
                                    }

                                    let ban_img_name = lookup_hero_image_by_text(&self.heroes, &ban_text);
                                    if !ban_img_name.is_empty() {
                                        if let Some(texture) = self.get_image(ctx, &ban_img_name) {
                                            ui.image((texture.id(), Vec2::splat(20.0)));
                                        } else {
                                            ui.label("...");
                                        }
                                    } else {
                                        ui.colored_label(
                                            Color32::from_rgb(200, 100, 100),
                                            lookup_hero_name_by_text(&self.heroes, &ban_text),
                                        );
                                    }

                                    ui.end_row();
                                }
                            });
                        });
                        ui.add_space(8.0);

                        // 使用 CollapsingHeader 构建器
                        egui::CollapsingHeader::new("日志")
                            .default_open(false) // 设置默认收起
                            .show(ui, |ui| {
                                egui::ScrollArea::vertical()
                                    .max_height(100.0)
                                    .stick_to_bottom(true)
                                    .show(ui, |ui| {
                                        for l in &self.log_lines {
                                            ui.colored_label(
                                                egui::Color32::from_rgb(150, 150, 150),
                                                l,
                                            );
                                        }
                                    });
                            });
                    });
                });
            });

        ctx.request_repaint_after(Duration::from_secs(2));
    }
}
