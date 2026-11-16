use super::*;

impl HexApp {

    /// 按当前输入过滤英雄（名称 / 称号 / 英文名），按 id 排序保证列表稳定
    pub(super) fn refresh_snipe_results(&mut self) {
        let term = self.search_input.trim().to_lowercase();
        let map = self.heroes.lock().unwrap();
        let mut results: Vec<Hero> = map
            .values()
            .filter(|h| term.is_empty() || h.alias.contains(&term))
            .cloned()
            .collect();
        results.sort_by_key(|h| h.id);
        self.search_result = results;
        self.snipe_highlight = self
            .snipe_highlight
            .min(self.search_result.len().saturating_sub(1));
    }

    pub(super) fn toggle_snipe(&mut self, id: i32) {
        if let Some(pos) = self.settings.snipe_list.iter().position(|&x| x == id) {
            self.settings.snipe_list.remove(pos);
        } else {
            self.settings.snipe_list.push(id);
        }
        self.trigger_save();
    }

    /// 大乱斗抢人：可搜索、多选、键盘操作的英雄选择器 + 已选列表
    pub(super) fn show_aram_snipe(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let edit_id = egui::Id::new("snipe_search_edit");
        let popup_id = egui::Id::new("snipe_search_popup");

        if self.snipe_open {
            self.refresh_snipe_results();
        }

        // 在 TextEdit 处理事件前消费导航键，避免与光标移动/失焦冲突
        let mut toggle_id: Option<i32> = None;
        let mut key_nav = false;
        if self.snipe_open {
            let count = self.search_result.len();
            ui.input_mut(|i| {
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) && count > 0 {
                    self.snipe_highlight = (self.snipe_highlight + 1).min(count - 1);
                    key_nav = true;
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) && count > 0 {
                    self.snipe_highlight = self.snipe_highlight.saturating_sub(1);
                    key_nav = true;
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
                    if let Some(h) = self.search_result.get(self.snipe_highlight) {
                        toggle_id = Some(h.id);
                    }
                }
            });
        }
        if let Some(id) = toggle_id {
            self.toggle_snipe(id);
        }

        let row_width = ui.available_width();
        let btn_w = 22.0;
        let anchor = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.search_input)
                        .id(edit_id)
                        .hint_text("搜索英雄 (名称 / 称号 / 英文)")
                        .clip_text(true)
                        // TextEdit 外框会在 desired_width 之外再加左右各 4px 内边距，
                        // 这里预先扣除，使整行（输入框+间隔+箭头）恰好等于可用宽度，
                        // 避免卡片内在宽度随 content_w 逐帧增长形成反馈螺旋。
                        .desired_width(row_width - btn_w - 4.0 - 8.0),
                );
                if resp.gained_focus() && !self.snipe_open {
                    self.snipe_open = true;
                    self.snipe_highlight = 0;
                }
                if resp.changed() {
                    self.snipe_open = true;
                    self.snipe_highlight = 0;
                    self.refresh_snipe_results();
                }
                // 清空 × 内嵌于输入框最右侧，仅在有文字时出现
                if !self.search_input.is_empty() {
                    let er = resp.rect;
                    let cr = egui::Rect::from_center_size(
                        egui::pos2(er.max.x - 11.0, er.center().y),
                        egui::vec2(16.0, 16.0),
                    );
                    let cresp =
                        ui.interact(cr, egui::Id::new("snipe_clear"), egui::Sense::click());
                    let cc = if cresp.hovered() {
                        Color32::WHITE
                    } else {
                        Color32::from_gray(150)
                    };
                    ui.painter().text(
                        cr.center(),
                        egui::Align2::CENTER_CENTER,
                        "×",
                        egui::FontId::proportional(15.0),
                        cc,
                    );
                    if cresp.hovered() {
                        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if cresp.clicked() {
                        self.search_input.clear();
                        self.snipe_highlight = 0;
                        self.snipe_open = true;
                        self.refresh_snipe_results();
                        ui.memory_mut(|m| m.request_focus(edit_id));
                    }
                }
                let arrow = ui.add(
                    egui::Button::new(
                        egui::RichText::new(if self.snipe_open { "▲" } else { "▼" }).size(10.0),
                    )
                    .min_size(egui::vec2(btn_w, btn_w)),
                );
                if arrow.clicked() {
                    self.snipe_open = !self.snipe_open;
                    self.snipe_highlight = 0;
                    if self.snipe_open {
                        self.refresh_snipe_results();
                        ui.memory_mut(|m| m.request_focus(edit_id));
                    }
                }
                resp.union(arrow)
            })
            .inner;

        if self.snipe_open {
            let mut pos = egui::pos2(anchor.rect.left(), anchor.rect.bottom() + 2.0);
            if let Some(t) = ctx.memory(|m| m.layer_transforms.get(&ui.layer_id()).copied()) {
                pos = t * pos;
            }
            let selected: HashSet<i32> = self.settings.snipe_list.iter().copied().collect();
            let row_h = 30.0;
            let area = egui::Area::new(popup_id)
                .order(egui::Order::Foreground)
                .fixed_pos(pos)
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(Color32::from_rgb(6, 16, 28))
                        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(120, 90, 40)))
                        .rounding(6.0)
                        .inner_margin(4.0)
                        .show(ui, |ui| {
                            ui.style_mut().interaction.selectable_labels = false;
                            let w = row_width - 8.0;
                            ui.set_min_width(w);
                            ui.set_max_width(w);
                            ui.spacing_mut().item_spacing.y = 0.0;
                            if self.search_result.is_empty() {
                                ui.add_space(6.0);
                                ui.colored_label(Color32::from_gray(140), "无匹配英雄");
                                ui.add_space(6.0);
                            } else {
                                egui::ScrollArea::vertical()
                                    .max_height(row_h * 8.0)
                                    .show_rows(ui, row_h, self.search_result.len(), |ui, range| {
                                        for i in range {
                                            let h = self.search_result[i].clone();
                                            let is_sel = selected.contains(&h.id);
                                            let is_hl = i == self.snipe_highlight;
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
                                            if resp.hovered() {
                                                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                                            }
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
                                                                        .fit_to_exact_size(egui::vec2(24.0, 24.0))
                                                                        .rounding(4.0),
                                                                );
                                                            } else {
                                                                ui.allocate_exact_size(
                                                                    egui::vec2(24.0, 24.0),
                                                                    egui::Sense::hover(),
                                                                );
                                                            }
                                                            ui.add_space(6.0);
                                                            let name_color = if is_sel {
                                                                Color32::from_rgb(220, 190, 120)
                                                            } else {
                                                                Color32::from_rgb(225, 230, 235)
                                                            };
                                                            ui.label(
                                                                egui::RichText::new(&h.title)
                                                                    .color(name_color)
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
                                                                egui::Layout::right_to_left(egui::Align::Center),
                                                                |ui| {
                                                                    if is_sel {
                                                                        ui.label(
                                                                            egui::RichText::new("✓")
                                                                                .color(Color32::from_rgb(220, 190, 120))
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
                                                self.toggle_snipe(h.id);
                                                ui.memory_mut(|m| m.request_focus(edit_id));
                                            }
                                            if key_nav && is_hl {
                                                resp.scroll_to_me(Some(egui::Align::Center));
                                            }
                                        }
                                    });
                            }
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new("↑↓ 移动 · Enter 选择/取消 · Esc 关闭")
                                    .size(10.5)
                                    .color(Color32::from_gray(120)),
                            );
                        });
                });
            let popup_rect = area.response.rect;
            let clicked_outside = ctx.input(|i| {
                i.pointer.any_click()
                    && i.pointer.interact_pos().map_or(false, |p| {
                        !popup_rect.contains(p) && !anchor.rect.contains(p)
                    })
            });
            if clicked_outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.snipe_open = false;
            }
        }

        ui.add_space(2.0);

        if self.settings.snipe_list.is_empty() {
            ui.colored_label(Color32::from_gray(120), "尚未添加英雄，按列表顺序优先抢选");
        } else {
            let mut rm: Option<usize> = None;
            let mut reorder: Option<(i32, i32)> = None;
            let ids: Vec<i32> = self.settings.snipe_list.clone();
            let col_gap = 6.0;
            let col_w = (ui.available_width() - col_gap) * 0.5;
            let row_h = 26.0;
            // 拖拽排序：整个网格为单一 drop zone，释放时按指针位置取最近单元格，放置更可靠
            let saved = ui.visuals().widgets.clone();
            {
                let w = &mut ui.visuals_mut().widgets;
                w.inactive.bg_fill = Color32::TRANSPARENT;
                w.inactive.weak_bg_fill = Color32::TRANSPARENT;
                w.inactive.bg_stroke = egui::Stroke::NONE;
                w.active.bg_fill = Color32::TRANSPARENT;
                w.active.weak_bg_fill = Color32::TRANSPARENT;
                w.active.bg_stroke = egui::Stroke::NONE;
            }
            let mut cell_rects: Vec<egui::Rect> = Vec::new();
            let (_outer, dropped) = ui.dnd_drop_zone::<i32, ()>(egui::Frame::none(), |ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                let rows = (ids.len() + 1) / 2;
                for row in 0..rows {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = col_gap;
                        for col in 0..2 {
                            let i = row * 2 + col;
                            if i >= ids.len() {
                                ui.allocate_exact_size(
                                    egui::vec2(col_w, row_h),
                                    egui::Sense::hover(),
                                );
                                continue;
                            }
                            let id = ids[i];
                            let hero = self.heroes.lock().unwrap().get(&id).cloned();
                            // 固定槽位：allocate_exact_size 精确推进游标，单元格宽高严格等于
                            // col_w × row_h，内容全部手绘于槽内（ui.interact 不推进游标），
                            // 从根本上保证双列对齐、行距规整，不会因内容高低不同而错位。
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(col_w, row_h),
                                egui::Sense::hover(),
                            );
                            cell_rects.push(rect);
                            let hovered = ui.rect_contains_pointer(rect);
                            if hovered {
                                ui.painter().rect_filled(
                                    rect,
                                    3.0,
                                    Color32::from_rgb(30, 45, 61),
                                );
                            }
                            let inner = rect.shrink2(egui::vec2(3.0, 1.0));
                            let cy = inner.center().y;
                            let name_w =
                                (col_w - 6.0 - 10.0 - 20.0 - 16.0 - 12.0).max(0.0);
                            // 拖拽手柄：左侧 10×16，垂直居中
                            let handle_rect = egui::Rect::from_min_size(
                                egui::pos2(inner.min.x, cy - 8.0),
                                egui::vec2(10.0, 16.0),
                            );
                            let handle_id = egui::Id::new(("snipe_handle", id));
                            let handle_resp =
                                ui.interact(handle_rect, handle_id, egui::Sense::drag());
                            if handle_resp.drag_started() {
                                egui::DragAndDrop::set_payload(ctx, id);
                            }
                            if handle_resp.hovered() {
                                ctx.set_cursor_icon(egui::CursorIcon::Grab);
                            }
                            let dot = Color32::from_gray(95);
                            for c in 0..2 {
                                for r in 0..3 {
                                    let p = egui::pos2(
                                        handle_rect.min.x + 2.5 + c as f32 * 5.0,
                                        handle_rect.min.y + 3.0 + r as f32 * 5.0,
                                    );
                                    ui.painter().circle_filled(p, 1.2, dot);
                                }
                            }
                            // 头像：手柄右 4px，20×20，垂直居中
                            let av_rect = egui::Rect::from_min_size(
                                egui::pos2(handle_rect.max.x + 4.0, cy - 10.0),
                                egui::vec2(20.0, 20.0),
                            );
                            if let Some(h) = &hero {
                                if let Some(tex) = self.get_image(ctx, &h.image_name) {
                                    let mut rs = egui::epaint::RectShape::new(
                                        av_rect,
                                        3.0,
                                        Color32::WHITE,
                                        egui::Stroke::NONE,
                                    );
                                    rs.fill_texture_id = tex.id();
                                    rs.uv = egui::Rect::from_min_max(
                                        egui::pos2(0.0, 0.0),
                                        egui::pos2(1.0, 1.0),
                                    );
                                    ui.painter().add(rs);
                                }
                            }
                            // 名称：头像右 4px，固定宽 name_w，单行截断
                            let name_rect = egui::Rect::from_min_size(
                                egui::pos2(av_rect.max.x + 4.0, cy - 10.0),
                                egui::vec2(name_w, 20.0),
                            );
                            let name_text = hero
                                .as_ref()
                                .map(|h| h.title.clone())
                                .unwrap_or_else(|| format!("英雄 {}", id));
                            let name_color = if hero.is_some() {
                                Color32::from_rgb(235, 238, 242)
                            } else {
                                Color32::GRAY
                            };
                            let galley = egui::WidgetText::from(
                                egui::RichText::new(name_text)
                                    .strong()
                                    .color(name_color)
                                    .size(12.0),
                            )
                            .into_galley(ui, None, f32::INFINITY, egui::TextStyle::Button);
                            ui.painter().with_clip_rect(name_rect).galley(
                                egui::pos2(name_rect.min.x, cy - galley.size().y * 0.5),
                                galley,
                                name_color,
                            );
                            // 头像左上角极小金色序号角标，消除双列顺序歧义
                            let badge = egui::Rect::from_min_size(
                                av_rect.min + egui::vec2(-3.0, -3.0),
                                egui::vec2(11.0, 11.0),
                            );
                            ui.painter().rect_filled(
                                badge,
                                2.0,
                                Color32::from_rgba_unmultiplied(8, 16, 26, 220),
                            );
                            ui.painter().text(
                                badge.center(),
                                egui::Align2::CENTER_CENTER,
                                format!("{}", i + 1),
                                egui::FontId::proportional(8.0),
                                Color32::from_rgb(200, 170, 110),
                            );
                            // 移除 ×：右侧 16×16，垂直居中，仅悬停时明显
                            let xr = egui::Rect::from_min_size(
                                egui::pos2(inner.max.x - 16.0, cy - 8.0),
                                egui::vec2(16.0, 16.0),
                            );
                            let xresp = ui.interact(
                                xr,
                                egui::Id::new(("snipe_rm", id)),
                                egui::Sense::click(),
                            );
                            let xc = if hovered {
                                Color32::from_rgb(235, 120, 120)
                            } else {
                                Color32::from_rgba_unmultiplied(160, 160, 160, 70)
                            };
                            ui.painter().text(
                                xr.center(),
                                egui::Align2::CENTER_CENTER,
                                "×",
                                egui::FontId::proportional(14.0),
                                xc,
                            );
                            if xresp.hovered() {
                                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if xresp.clicked() {
                                rm = Some(i);
                            }
                        }
                    });
                }
            });
            ui.visuals_mut().widgets = saved;
            if let Some(i) = rm {
                self.settings.snipe_list.remove(i);
                self.trigger_save();
            }
            if let Some(src) = dropped {
                if let Some(p) = ctx.input(|i| i.pointer.interact_pos()) {
                    let target = cell_rects
                        .iter()
                        .position(|r| r.contains(p))
                        .unwrap_or_else(|| {
                            cell_rects
                                .iter()
                                .enumerate()
                                .min_by_key(|(_, r)| {
                                    let d = p - r.center();
                                    ((d.x * d.x + d.y * d.y) * 100.0) as i64
                                })
                                .map(|(i, _)| i)
                                .unwrap_or(0)
                        });
                    if let Some(&tid) = ids.get(target) {
                        reorder = Some((*src, tid));
                    }
                }
            }
            if let Some((src_id, target_id)) = reorder {
                self.reorder_snipe(src_id, target_id);
                self.trigger_save();
            }
        }
    }

    pub(super) fn reorder_snipe(&mut self, src_id: i32, target_id: i32) {
        if src_id == target_id {
            return;
        }
        let list = &mut self.settings.snipe_list;
        let Some(from) = list.iter().position(|&x| x == src_id) else {
            return;
        };
        let item = list.remove(from);
        let to = list
            .iter()
            .position(|&x| x == target_id)
            .unwrap_or(list.len());
        list.insert(to, item);
    }
}
