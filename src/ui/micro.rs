use super::*;


/// 微缩模式标题栏的极简对局/房间状态文本与颜色
fn micro_status(connected: bool, in_ready_check: bool, phase: &str) -> (&'static str, Color32) {
    if !connected {
        ("未连接", Color32::from_rgb(255, 100, 100))
    } else if in_ready_check {
        ("准备中", Color32::from_rgb(255, 120, 120))
    } else {
        match phase {
            "InProgress" | "GameStart" => ("游戏中", Color32::from_rgb(100, 230, 180)),
            "ChampSelect" => ("选人中", Color32::from_rgb(100, 200, 230)),
            "Matchmaking" => ("匹配中", Color32::from_rgb(180, 180, 230)),
            "Lobby" => ("房间中", Color32::from_rgb(10, 203, 230)),
            _ => ("已连接", Color32::from_rgb(10, 203, 230)),
        }
    }
}

/// 微缩标题栏窗口控制图标（矢量绘制，避免字体字形缺失/错位）
#[derive(Clone, Copy)]
enum MicroIcon {
    Restore,
    Min,
    Close,
}

fn micro_icon_button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    kind: MicroIcon,
    fg: Color32,
    hover: Color32,
) -> egui::Response {
    let resp = ui.interact(
        rect,
        egui::Id::new(("micro_icon", kind as u8)),
        egui::Sense::click(),
    );
    let bg = if resp.hovered() {
        hover
    } else {
        Color32::TRANSPARENT
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, bg);
    let c = rect.center();
    let stroke = egui::Stroke::new(1.4, if resp.hovered() { Color32::WHITE } else { fg });
    match kind {
        MicroIcon::Min => {
            painter.line_segment(
                [egui::pos2(c.x - 5.0, c.y), egui::pos2(c.x + 5.0, c.y)],
                stroke,
            );
        }
        MicroIcon::Close => {
            painter.line_segment(
                [egui::pos2(c.x - 4.5, c.y - 4.5), egui::pos2(c.x + 4.5, c.y + 4.5)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(c.x - 4.5, c.y + 4.5), egui::pos2(c.x + 4.5, c.y - 4.5)],
                stroke,
            );
        }
        MicroIcon::Restore => {
            // 参考图：上下两条方向相反的箭头（交换/还原），按 24 viewBox 映射到热区
            let s = rect.width() / 24.0;
            let p = |x: f32, y: f32| rect.min + egui::vec2(x * s, y * s);
            painter.line_segment([p(20.5, 14.99), p(15.49, 20.01)], stroke);
            painter.line_segment([p(3.5, 14.99), p(20.5, 14.99)], stroke);
            painter.line_segment([p(3.5, 9.01), p(8.51, 3.99)], stroke);
            painter.line_segment([p(20.5, 9.01), p(3.5, 9.01)], stroke);
        }
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}
impl HexApp {

    /// 微缩模式标题栏：左侧恢复窗口图标 + 极简对局状态，右侧最小化/退出，不显示应用名
    pub(super) fn render_micro_title(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        title_bar_rect: egui::Rect,
    ) {
        ui.style_mut().interaction.selectable_labels = false;
        let right_w = 62.0;
        let right_rect = egui::Rect::from_min_max(
            egui::pos2(title_bar_rect.max.x - right_w, title_bar_rect.min.y),
            title_bar_rect.max,
        );
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(right_rect), |ui| {
            let cy = right_rect.center().y;
            let close_rect = egui::Rect::from_min_size(
                egui::pos2(right_rect.max.x - 28.0, cy - 11.0),
                egui::vec2(22.0, 22.0),
            );
            let min_rect = egui::Rect::from_min_size(
                egui::pos2(close_rect.min.x - 26.0, cy - 11.0),
                egui::vec2(22.0, 22.0),
            );
            if micro_icon_button(
                ui,
                close_rect,
                MicroIcon::Close,
                Color32::from_gray(220),
                Color32::from_rgb(150, 40, 40),
            )
            .clicked()
            {
                self.trigger_save();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if micro_icon_button(
                ui,
                min_rect,
                MicroIcon::Min,
                Color32::from_gray(200),
                Color32::from_rgb(40, 55, 70),
            )
            .clicked()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        });
        let left_rect = egui::Rect::from_min_max(
            title_bar_rect.min + egui::vec2(4.0, 0.0),
            egui::pos2(title_bar_rect.max.x - right_w - 2.0, title_bar_rect.max.y),
        );
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(left_rect), |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add_space(2.0);
                let (slot, _) =
                    ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                if micro_icon_button(
                    ui,
                    slot,
                    MicroIcon::Restore,
                    Color32::from_rgb(205, 190, 145),
                    Color32::from_rgb(40, 55, 70),
                )
                .clicked()
                {
                    self.settings.micro_mode = false;
                    self.trigger_save();
                }
                ui.add_space(4.0);
                let (txt, col) =
                    micro_status(self.connected, self.in_ready_check, &self.current_phase);
                ui.colored_label(col, egui::RichText::new(txt).size(12.0));
            });
        });
    }

    /// 微缩模式内容：仅 4 个高频硬开关（暂停接管/自动接受/自动抢人/自动匹配，无秒数框），
    /// 卡片只保留可操作按钮文本，无标题与提示；大乱斗时附板凳席头像按钮，否则隐藏。
    pub(super) fn render_micro_content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> egui::Vec2 {
        let micro_w = 220.0;
        let inner_w = micro_w - 12.0;
        ui.set_width(micro_w);
        let frame = egui::Frame::none()
            .fill(Color32::from_rgb(10, 25, 40))
            .inner_margin(6.0)
            .rounding(4.0)
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(120, 90, 40)));
        frame.show(ui, |ui| {
            ui.set_width(inner_w);
            let gap = 6.0;
            let col_w = (inner_w - gap) / 2.0;
            let row_h = 20.0;
            let origin = ui.cursor().min;
            let grid = egui::Rect::from_min_size(
                origin,
                egui::vec2(inner_w, row_h * 2.0 + gap),
            );
            let cell_rect = |col: f32, row: f32| {
                egui::Rect::from_min_size(
                    origin + egui::vec2(col * (col_w + gap), row * (row_h + gap)),
                    egui::vec2(col_w, row_h),
                )
            };
            let centered = egui::Layout::top_down(egui::Align::Center);
            // 第一行左：暂停接管胶囊（文本居中）
            ui.allocate_new_ui(
                egui::UiBuilder::new().max_rect(cell_rect(0.0, 0.0)).layout(centered),
                |ui| {
                    let (label, base, fg) = if self.settings.pause_all {
                        (
                            "已暂停",
                            Color32::from_rgb(96, 62, 24),
                            Color32::from_rgb(245, 196, 110),
                        )
                    } else {
                        (
                            "接管中",
                            Color32::from_rgb(24, 66, 50),
                            Color32::from_rgb(150, 235, 190),
                        )
                    };
                    if state_pill(
                        ui,
                        egui::RichText::new(label).size(11.5).strong().color(fg),
                        base,
                        egui::vec2(col_w, 18.0),
                    )
                    .clicked()
                    {
                        self.settings.pause_all = !self.settings.pause_all;
                        self.trigger_save();
                    }
                },
            );
            // 第一行右：自动接受
            ui.allocate_new_ui(
                egui::UiBuilder::new().max_rect(cell_rect(1.0, 0.0)).layout(centered),
                |ui| {
                    if hex_checkbox(ui, &mut self.settings.auto_accept, "自动接受").changed() {
                        self.trigger_save();
                    }
                },
            );
            // 第二行左：自动抢人
            ui.allocate_new_ui(
                egui::UiBuilder::new().max_rect(cell_rect(0.0, 1.0)).layout(centered),
                |ui| {
                    if hex_checkbox(ui, &mut self.settings.aram_snipe, "自动抢人").changed() {
                        self.trigger_save();
                    }
                },
            );
            // 第二行右：自动匹配
            ui.allocate_new_ui(
                egui::UiBuilder::new().max_rect(cell_rect(1.0, 1.0)).layout(centered),
                |ui| {
                    if hex_checkbox(ui, &mut self.settings.auto_queue, "自动匹配").changed() {
                        self.trigger_save();
                    }
                },
            );
            ui.advance_cursor_after_rect(grid);
        });

        if self.is_aram && !self.bench_ids.is_empty() {
            ui.add_space(4.0);
            let bench_data: Vec<Hero> = {
                if let Ok(map) = self.heroes.lock() {
                    self.bench_ids
                        .iter()
                        .filter_map(|id| map.get(id).cloned())
                        .collect()
                } else {
                    Vec::new()
                }
            };
            frame.show(ui, |ui| {
                ui.set_width(inner_w);
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                // 5 等分槽位（space-around）：整行头像铺满容器宽度，
                // 不足一行时按相同槽位居左排列
                let pad = ui.spacing().button_padding;
                let item_w = 28.0 + 2.0 * pad.x;
                let slot = inner_w / 5.0;
                let gap = (slot - item_w).max(0.0);
                for chunk in bench_data.chunks(5) {
                    ui.horizontal(|ui| {
                        ui.add_space(gap * 0.5);
                        ui.spacing_mut().item_spacing.x = gap;
                        for h in chunk {
                            if let Some(texture) = self.get_image(ctx, &h.image_name) {
                                let img = egui::Image::new(&texture)
                                    .fit_to_exact_size(egui::Vec2::splat(28.0));
                                if ui
                                    .add(egui::Button::image(img).rounding(4.0))
                                    .clicked()
                                {
                                    let _ = self.tx_to_backend.send(BackendMsg::SwapChamp(h.id));
                                }
                            }
                        }
                    });
                }
            });
        }
        ui.min_rect().size()
    }
}
