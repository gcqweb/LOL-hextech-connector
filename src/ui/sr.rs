use super::*;

impl HexApp {

    /// 峡谷/排位 紧凑胶囊：头像+英雄名合一，hover 金边；点击返回胶囊矩形以唤起选择浮窗。
    pub(super) fn sr_capsule(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        pos: &str,
        is_ban: bool,
        enabled: bool,
        width: f32,
    ) -> Option<egui::Rect> {
        let id = if is_ban {
            self.settings.sr_bans.get(pos).copied().unwrap_or(0)
        } else {
            self.settings.sr_picks.get(pos).copied().unwrap_or(0)
        };
        let hero = if id != 0 {
            self.heroes.lock().unwrap().get(&id).cloned()
        } else {
            None
        };
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 22.0), egui::Sense::click());
        let hovered = resp.hovered();
        let border = if hovered {
            Color32::from_rgb(200, 170, 110)
        } else {
            Color32::from_rgb(60, 74, 90)
        };
        ui.painter()
            .rect(rect, 3.0, Color32::from_rgb(16, 26, 38), egui::Stroke::new(1.0, border));
        let av = egui::Rect::from_min_size(rect.min + egui::vec2(3.0, 2.0), egui::vec2(18.0, 18.0));
        if let Some(h) = &hero {
            if let Some(tex) = self.get_image(ctx, &h.image_name) {
                ui.painter().image(
                    tex.id(),
                    av,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        } else {
            ui.painter().rect_stroke(av, 2.0, egui::Stroke::new(1.0, Color32::from_gray(70)));
        }
        let name = hero
            .as_ref()
            .map(|h| h.title.clone())
            .unwrap_or_else(|| "未设置".into());
        let nc = if hero.is_none() {
            Color32::from_gray(120)
        } else if is_ban {
            Color32::from_rgb(224, 108, 117) // 柔和珊瑚红
        } else {
            Color32::from_rgb(222, 226, 230)
        };
        let galley = egui::WidgetText::from(egui::RichText::new(name).color(nc).size(12.0))
            .into_galley(ui, None, width - 30.0, egui::TextStyle::Button);
        ui.painter().with_clip_rect(rect).galley(
            egui::pos2(av.max.x + 5.0, rect.center().y - galley.size().y * 0.5),
            galley,
            nc,
        );
        if !enabled {
            ui.painter()
                .rect_filled(rect, 3.0, Color32::from_rgba_unmultiplied(10, 20, 30, 150));
        }
        if hovered {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            Some(rect)
        } else {
            None
        }
    }

    pub(super) fn apply_sr_pick(&mut self, pos: &str, is_ban: bool, id: i32) {
        if is_ban {
            self.settings.sr_bans.insert(pos.to_string(), id);
        } else {
            self.settings.sr_picks.insert(pos.to_string(), id);
        }
        self.trigger_save();
    }

    pub(super) fn refresh_picker_results(&mut self) {
        let q = self.picker_query.trim().to_lowercase();
        let map = self.heroes.lock().unwrap();
        let mut v: Vec<Hero> = map
            .values()
            .filter(|h| {
                q.is_empty()
                    || h.title.to_lowercase().contains(&q)
                    || h.name.to_lowercase().contains(&q)
                    || h.alias.contains(&q)
            })
            .cloned()
            .collect();
        v.sort_by_key(|h| h.id);
        self.picker_results = v;
    }

    /// 单例英雄选择浮窗：复用大乱斗搜索面板的交互（搜索/键盘/头像行），单选写回峡谷/排位。
    pub(super) fn show_sr_picker(&mut self, ctx: &egui::Context) {
        let Some((pos, is_ban)) = self.sr_picker.clone() else {
            return;
        };
        let edit_id = egui::Id::new("sr_picker_edit");
        let popup_id = egui::Id::new("sr_picker_popup");
        let count = self.picker_results.len();
        let mut pick: Option<i32> = None;
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) && count > 0 {
                self.picker_highlight = (self.picker_highlight + 1).min(count - 1);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) && count > 0 {
                self.picker_highlight = self.picker_highlight.saturating_sub(1);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
                if let Some(h) = self.picker_results.get(self.picker_highlight) {
                    pick = Some(h.id);
                }
            }
        });
        if let Some(id) = pick {
            self.apply_sr_pick(&pos, is_ban, id);
            self.sr_picker = None;
            return;
        }
        if !ctx.memory(|m| m.has_focus(edit_id)) {
            ctx.memory_mut(|m| m.request_focus(edit_id));
        }
        let selected_id = if is_ban {
            self.settings.sr_bans.get(&pos).copied().unwrap_or(0)
        } else {
            self.settings.sr_picks.get(&pos).copied().unwrap_or(0)
        };
        let anchor = self.picker_anchor;
        let row_h = 28.0;
        let width = 210.0;
        let area = egui::Area::new(popup_id)
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(anchor.left(), anchor.bottom() + 2.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(Color32::from_rgb(6, 16, 28))
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(120, 90, 40)))
                    .rounding(6.0)
                    .inner_margin(4.0)
                    .show(ui, |ui| {
                        ui.set_min_width(width);
                        ui.set_max_width(width);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.picker_query)
                                .id(edit_id)
                                .hint_text("搜索英雄 (名称 / 称号 / 英文)")
                                .desired_width(width - 8.0),
                        );
                        if resp.changed() {
                            self.picker_highlight = 0;
                            self.refresh_picker_results();
                        }
                        ui.add_space(2.0);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        if self.picker_results.is_empty() {
                            ui.colored_label(Color32::from_gray(140), "无匹配英雄");
                        } else {
                            egui::ScrollArea::vertical()
                                .max_height(row_h * 7.0)
                                .show_rows(ui, row_h, self.picker_results.len(), |ui, range| {
                                    for i in range {
                                        let h = self.picker_results[i].clone();
                                        let is_sel = h.id == selected_id;
                                        let is_hl = i == self.picker_highlight;
                                        let (rect, resp) = ui.allocate_exact_size(
                                            egui::vec2(ui.available_width(), row_h),
                                            egui::Sense::click(),
                                        );
                                        let bg = if is_hl {
                                            Color32::from_rgb(0, 62, 92)
                                        } else if resp.hovered() {
                                            Color32::from_rgb(18, 32, 46)
                                        } else {
                                            Color32::TRANSPARENT
                                        };
                                        ui.painter().rect_filled(rect, 4.0, bg);
                                        ui.allocate_new_ui(
                                            egui::UiBuilder::new()
                                                .max_rect(rect.shrink2(egui::vec2(4.0, 2.0))),
                                            |ui| {
                                                ui.with_layout(
                                                    egui::Layout::left_to_right(egui::Align::Center),
                                                    |ui| {
                                                        if let Some(tex) =
                                                            self.get_image(ctx, &h.image_name)
                                                        {
                                                            ui.add(
                                                                egui::Image::new(&tex)
                                                                    .fit_to_exact_size(
                                                                        egui::vec2(22.0, 22.0),
                                                                    )
                                                                    .rounding(3.0),
                                                            );
                                                        }
                                                        ui.add_space(6.0);
                                                        let nc = if is_sel {
                                                            Color32::from_rgb(220, 190, 120)
                                                        } else {
                                                            Color32::from_rgb(225, 230, 235)
                                                        };
                                                        ui.label(
                                                            egui::RichText::new(&h.title)
                                                                .color(nc)
                                                                .strong(),
                                                        );
                                                        ui.add(
                                                            egui::Label::new(
                                                                egui::RichText::new(&h.name)
                                                                    .size(11.0)
                                                                    .color(Color32::from_gray(130)),
                                                            )
                                                            .truncate(),
                                                        );
                                                        ui.with_layout(
                                                            egui::Layout::right_to_left(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                if is_sel {
                                                                    ui.label(
                                                                        egui::RichText::new("✓")
                                                                            .color(Color32::from_rgb(
                                                                                220, 190, 120,
                                                                            ))
                                                                            .strong(),
                                                                    );
                                                                }
                                                            },
                                                        );
                                                    },
                                                );
                                            },
                                        );
                                        if resp.clicked() {
                                            pick = Some(h.id);
                                        }
                                        if is_hl {
                                            resp.scroll_to_me(Some(egui::Align::Center));
                                        }
                                    }
                                });
                        }
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new("↑↓ 移动 · Enter 选择 · Esc 关闭")
                                .size(10.5)
                                .color(Color32::from_gray(120)),
                        );
                    });
            });
        if let Some(id) = pick {
            self.apply_sr_pick(&pos, is_ban, id);
            self.sr_picker = None;
            return;
        }
        let pr = area.response.rect;
        let outside = ctx.input(|i| {
            i.pointer.any_click()
                && i.pointer
                    .interact_pos()
                    .map_or(false, |p| !pr.contains(p) && !anchor.contains(p))
        });
        if outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.sr_picker = None;
        }
    }
}
