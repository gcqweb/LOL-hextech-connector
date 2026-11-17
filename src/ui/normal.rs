use super::*;

impl HexApp {
    /// 完整模式标题栏：右侧窗口控制 + 状态按钮，左侧应用大标题（裁剪防遮挡）
    pub(super) fn render_normal_title(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        title_bar_rect: egui::Rect,
    ) {
        ui.style_mut().interaction.selectable_labels = false; // Disable text selection
        let right_w = 150.0;
        let right_rect = egui::Rect::from_min_max(
            egui::pos2(title_bar_rect.max.x - right_w, title_bar_rect.min.y),
            title_bar_rect.max,
        );
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(right_rect), |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(6.0);

                // Close Button：平时透明，hover 暗红
                if title_button(
                    ui,
                    "×",
                    Color32::from_gray(220),
                    Color32::from_rgb(150, 40, 40),
                )
                .clicked()
                {
                    // 关闭前立即落盘，确保最新窗口位置不因防抖窗口未到期而丢失
                    self.trigger_save();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }

                // Minimize Button：hover 微光蓝灰
                if title_button(
                    ui,
                    "-",
                    Color32::from_gray(200),
                    Color32::from_rgb(40, 55, 70),
                )
                .clicked()
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }

                ui.add_space(6.0);
                //↻重连：hover 微光蓝灰
                if title_button(
                    ui,
                    "↻",
                    Color32::from_gray(200),
                    Color32::from_rgb(40, 55, 70),
                )
                .clicked()
                {
                    let _ = self.tx_to_backend.send(BackendMsg::ForceReconnect);
                }
                            
                // 统一状态按钮 - 根据状态显示不同文字和颜色
                let (btn_text, btn_color, text_color) = if !self.connected {
                    // 未连接 - 红色，点击启动客户端
                    ("未连接", Color32::from_rgb(80, 30, 30), Color32::from_rgb(255, 100, 100))
                } else if self.in_ready_check {
                    // 准备检查中 - 红色警告，点击取消
                    ("取消接受", Color32::from_rgb(200, 50, 50), Color32::WHITE)
                } else {
                    // 根据阶段显示不同状态
                    match self.current_phase.as_str() {
                        "InProgress" | "GameStart" => {
                            ("游戏中", Color32::from_rgb(20, 80, 60), Color32::from_rgb(100, 230, 180))
                        }
                        "ChampSelect" => {
                            ("选人中", Color32::from_rgb(20, 60, 80), Color32::from_rgb(100, 200, 230))
                        }
                        "Matchmaking" => {
                            ("匹配中", Color32::from_rgb(50, 50, 80), Color32::from_rgb(180, 180, 230))
                        }
                        "Lobby" => {
                            ("房间中", Color32::from_rgb(20, 50, 60), Color32::from_rgb(10, 203, 230))
                        }
                        _ => {
                            ("已连接", Color32::from_rgb(20, 40, 50), Color32::from_rgb(10, 203, 230))
                        }
                    }
                };
                            
                let status_btn = egui::Button::new(
                    egui::RichText::new(btn_text).color(text_color)
                ).fill(btn_color).rounding(4.0)
                .stroke(egui::Stroke::new(1.0, btn_color.gamma_multiply(1.8)));
                            
                if ui.add(status_btn).clicked() {
                    if !self.connected {
                        // 未连接时启动客户端
                        // let _ = self.tx_to_backend.send(BackendMsg::LaunchClient);
                        let _ = self.tx_to_backend.send(BackendMsg::ForceReconnect);
                    } else if self.in_ready_check {
                        // 准备检查中取消接受
                        let _ = self.tx_to_backend.send(BackendMsg::DeclineReadyCheck);
                    }
                }
            });
        });
        // 左侧大标题：裁剪到右侧控件之外的区域，保证完整可见不被遮挡
        let left_rect = egui::Rect::from_min_max(
            title_bar_rect.min + egui::vec2(6.0, 0.0),
            egui::pos2(title_bar_rect.max.x - right_w - 4.0, title_bar_rect.max.y),
        );
        let title_color = egui::Color32::from_rgb(205, 190, 145);
        let galley = egui::WidgetText::from(
            egui::RichText::new(crate::utils::APP_TITLE)
                .size(16.0)
                .strong()
                .color(title_color),
        )
        .into_galley(ui, None, f32::INFINITY, egui::TextStyle::Button);
        ui.painter().with_clip_rect(left_rect).galley(
            egui::pos2(
                left_rect.min.x,
                title_bar_rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            title_color,
        );
    }

    /// 完整模式内容：板凳席 / 全局功能 / 大乱斗抢人 / 峡谷排位 / 日志门禁卡片
    pub(super) fn render_normal_content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> egui::Vec2 {
        let sr_label_w = 26.0;
        let sr_gap = 6.0;
        let sr_cap_w = (self.content_w - sr_label_w - 2.0 * sr_gap) * 0.5;
        // 不使用 ScrollArea：窗口高度自适应内容，垂直滚动条完全不需要
        let frame_style = egui::Frame::none()
                .fill(Color32::from_rgb(10, 25, 40)) // Slightly lighter blue for cards
                .inner_margin(6.0)
                .rounding(4.0)
                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(120, 90, 40)));

            frame_style.show(ui, |ui| {
                ui.set_width(self.content_w);
                ui.label(
                    egui::RichText::new("大乱斗板凳席")
                        .color(Color32::from_rgb(200, 170, 110))
                        .strong(),
                );
                if self.bench_ids.is_empty() {
                    ui.colored_label(Color32::GRAY, "等待队列中...");
                } else {
                    // 先收集英雄数据，然后释放锁
                    let bench_data: Vec<Hero> = {
                        if let Ok(map) = self.heroes.lock() {
                            self.bench_ids
                                .iter()
                                .filter_map(|id| map.get(id).cloned())
                                .collect()
                        } else {
                            Vec::new()
                        }
                    }; // map 在这里被释放
                    // 5 等分槽位（space-around）：整行头像铺满容器宽度，
                    // 不足一行时按相同槽位居左排列
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                    let pad = ui.spacing().button_padding;
                    let slot = self.content_w / 5.0;
                    // 大窗头像随槽位放大以适配宽度：槽宽扣除按钮内边距与固定槽间留白
                    let img_size = (slot - 2.0 * pad.x - 8.0).clamp(32.0, 56.0);
                    let item_w = img_size + 2.0 * pad.x;
                    let gap = (slot - item_w).max(0.0);
                    for chunk in bench_data.chunks(5) {
                        ui.horizontal(|ui| {
                            ui.add_space(gap * 0.5);
                            ui.spacing_mut().item_spacing.x = gap;
                            for h in chunk {
                                if let Some(texture) = self.get_image(ctx, &h.image_name) {
                                    let img = egui::Image::new(&texture)
                                        .fit_to_exact_size(Vec2::splat(img_size));
                                    if ui.add(egui::Button::image(img)).clicked() {
                                        let _ = self
                                            .tx_to_backend
                                            .send(BackendMsg::SwapChamp(h.id));
                                    }
                                }
                            }
                        });
                    }
                }
            });
            ui.add_space(4.0);

            frame_style.show(ui, |ui| {
                ui.set_width(self.content_w);
                // 标题行：左标题 + 右侧主控胶囊开关（接管中=绿 / 已暂停=琥珀）
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("全局功能")
                            .color(Color32::from_rgb(200, 170, 110))
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (label, base, fg) = if self.settings.pause_all {
                            ("已暂停", Color32::from_rgb(96, 62, 24), Color32::from_rgb(245, 196, 110))
                        } else {
                            ("接管中", Color32::from_rgb(24, 66, 50), Color32::from_rgb(150, 235, 190))
                        };
                        if state_pill(
                            ui,
                            egui::RichText::new(label).size(11.5).strong().color(fg),
                            base,
                            egui::vec2(64.0, 18.0),
                        )
                        .on_hover_text("点击切换：暂停后所有自动化失效（配置保留），再点恢复")
                        .clicked()
                        {
                            self.settings.pause_all = !self.settings.pause_all;
                            self.trigger_save();
                        }
                    });
                });
                ui.add_space(2.0);
                // 自动化流程：双列网格严格对齐
                ui.add_enabled_ui(!self.settings.pause_all, |ui| {
                    egui::Grid::new("global_grid")
                        .num_columns(2)
                        .spacing([12.0, 4.0])
                        .show(ui, |ui| {
                            if hex_checkbox(ui, &mut self.settings.auto_accept, "自动接受").changed() {
                                self.trigger_save();
                            }
                            if hex_checkbox(ui, &mut self.settings.auto_honor, "自动点赞").changed() {
                                self.trigger_save();
                            }
                            ui.end_row();
                            if hex_checkbox(ui, &mut self.settings.play_again, "自动回房间").changed() {
                                self.trigger_save();
                            }
                            ui.horizontal(|ui| {
                                if hex_checkbox(ui, &mut self.settings.auto_queue, "自动匹配").changed() {
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
                            ui.end_row();
                        });
                });
                // 社交伪装分组：伪装段位 + 隐身挂机
                gold_divider(ui);
                ui.horizontal(|ui| {
                    if hex_checkbox(ui, &mut self.settings.spoof_rank, "伪装段位").changed() {
                        self.trigger_save();
                        let _ = self.tx_to_backend.send(BackendMsg::UpdateRank);
                    }
                    egui::ComboBox::from_id_salt("tier")
                        .width(118.0)
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
                                    .selectable_value(&mut self.settings.spoof_tier, k.to_string(), v)
                                    .clicked()
                                {
                                    self.trigger_save();
                                    let _ = self.tx_to_backend.send(BackendMsg::UpdateRank);
                                }
                            }
                        });
                    egui::ComboBox::from_id_salt("div")
                        .width(50.0)
                        .selected_text(&self.settings.spoof_div)
                        .show_ui(ui, |ui| {
                            for d in ["IV", "III", "II", "I"] {
                                if ui
                                    .selectable_value(&mut self.settings.spoof_div, d.to_string(), d)
                                    .clicked()
                                {
                                    self.trigger_save();
                                    let _ = self.tx_to_backend.send(BackendMsg::UpdateRank);
                                }
                            }
                        });
                });
                // 隐身挂机：单一复选框直接控制并持久化；不受暂停接管影响
                ui.horizontal(|ui| {
                    if hex_checkbox(ui, &mut self.settings.invisible, "隐身挂机")
                        .on_hover_text("启用后好友列表显示你离线，可安心挂机；关闭恢复在线")
                        .changed()
                    {
                        self.trigger_save();
                        if self.connected {
                            let _ = self
                                .tx_to_backend
                                .send(BackendMsg::SetInvisible(self.settings.invisible));
                        }
                    }
                });
            });
            ui.add_space(4.0);

            frame_style.show(ui, |ui| {
                ui.set_width(self.content_w);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("大乱斗抢人")
                            .color(Color32::from_rgb(200, 170, 110))
                            .strong(),
                    );
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            ui.add_enabled_ui(!self.settings.pause_all, |ui| {
                                if hex_checkbox(
                                    ui,
                                    &mut self.settings.aram_snipe,
                                    "启用秒选",
                                )
                                .changed()
                                {
                                    self.trigger_save();
                                }
                            });
                        },
                    );
                });
                self.show_aram_snipe(ui, ctx);
            });
            ui.add_space(4.0);

            frame_style.show(ui, |ui| {
                ui.set_width(self.content_w);
                ui.label(
                    egui::RichText::new("峡谷/排位")
                        .color(Color32::from_rgb(200, 170, 110))
                        .strong(),
                );
                ui.add_enabled_ui(!self.settings.pause_all, |ui| {
                    ui.horizontal(|ui| {
                        if hex_checkbox(ui, &mut self.settings.sr_enable, "预选").changed() {
                            self.trigger_save();
                        }
                        if hex_checkbox(ui, &mut self.settings.auto_lock, "锁定").changed() {
                            self.trigger_save();
                        }
                        if countdown(ui, self.settings.auto_lock, &mut self.settings.lock_time) {
                            self.trigger_save();
                        }
                    });
                    ui.horizontal(|ui| {
                        if hex_checkbox(ui, &mut self.settings.sr_ban_enable, "禁用")
                            .changed()
                        {
                            self.trigger_save();
                        }
                        if hex_checkbox(ui, &mut self.settings.auto_ban_lock, "锁定").changed() {
                            self.trigger_save();
                        }
                        if countdown(ui, self.settings.auto_ban_lock, &mut self.settings.ban_time) {
                            self.trigger_save();
                        }
                    });
                });
                let pick_on = self.settings.sr_enable;
                let ban_on = self.settings.sr_ban_enable;
                // 手动 horizontal 行（而非 Grid）：Grid 会把单元格居中锚定，
                // 导致胶囊超出可用宽度被裁切。这里每行宽度严格等于 content_w。
                let row_h = 22.0;
                let header_font = egui::FontId::proportional(12.0);
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = sr_gap;
                    let (r, _) = ui.allocate_exact_size(
                        egui::vec2(sr_label_w, row_h),
                        egui::Sense::hover(),
                    );
                    ui.painter().text(
                        r.left_center(),
                        egui::Align2::LEFT_CENTER,
                        "位置",
                        header_font.clone(),
                        Color32::from_gray(150),
                    );
                    for h in ["预选", "禁用"] {
                        let (r, _) = ui.allocate_exact_size(
                            egui::vec2(sr_cap_w, row_h),
                            egui::Sense::hover(),
                        );
                        ui.painter().text(
                            r.center(),
                            egui::Align2::CENTER_CENTER,
                            h,
                            header_font.clone(),
                            Color32::from_gray(150),
                        );
                    }
                });
                let pos = vec![
                    ("top", "上"),
                    ("jungle", "野"),
                    ("middle", "中"),
                    ("bottom", "下"),
                    ("utility", "辅"),
                ];
                for (k, l) in pos {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = sr_gap;
                        let (r, _) = ui.allocate_exact_size(
                            egui::vec2(sr_label_w, row_h),
                            egui::Sense::hover(),
                        );
                        ui.painter().text(
                            r.left_center(),
                            egui::Align2::LEFT_CENTER,
                            l,
                            egui::FontId::proportional(12.0),
                            Color32::from_gray(210),
                        );
                        if let Some(r) =
                            self.sr_capsule(ui, ctx, k, false, pick_on, sr_cap_w)
                        {
                            self.sr_picker = Some((k.to_string(), false));
                            self.picker_anchor = r;
                            self.picker_query.clear();
                            self.picker_highlight = 0;
                            self.refresh_picker_results();
                        }
                        if let Some(r) =
                            self.sr_capsule(ui, ctx, k, true, ban_on, sr_cap_w)
                        {
                            self.sr_picker = Some((k.to_string(), true));
                            self.picker_anchor = r;
                            self.picker_query.clear();
                            self.picker_highlight = 0;
                            self.refresh_picker_results();
                        }
                    });
                }
            });

            // 日志门禁：默认隐藏，仅当配置文件 show_log=true 解锁后才显示
            if self.settings.show_log {
                ui.add_space(4.0);
                if hex_checkbox(
                    ui,
                    &mut self.settings.show_log,
                    egui::RichText::new("显示日志")
                        .size(11.0)
                        .color(Color32::from_gray(150)),
                )
                .changed()
                {
                    self.trigger_save();
                }
                egui::ScrollArea::vertical()
                    .max_height(100.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for l in &self.log_lines {
                            ui.colored_label(Color32::from_rgb(150, 150, 150), l);
                        }
                    });
            }
        ui.min_rect().size()
    }
}
