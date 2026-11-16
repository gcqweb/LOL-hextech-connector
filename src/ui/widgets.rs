use super::*;


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
    visuals.widgets.inactive.rounding = egui::Rounding::same(3.0);

    // Hovered
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(0, 90, 130); // Hextech Interact Blue
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, Color32::WHITE); // Force White Text
    visuals.widgets.hovered.rounding = egui::Rounding::same(3.0);

    // Active (Click / Drag / Checked)
    visuals.widgets.active.bg_fill = Color32::from_rgb(200, 170, 110); // Hextech Gold
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.rounding = egui::Rounding::same(3.0);

    // Open (Dropdowns)
    visuals.widgets.open.bg_fill = Color32::from_rgb(10, 25, 40);
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, Color32::WHITE);

    // Selection (Text select)
    visuals.selection.bg_fill = Color32::from_rgb(0, 90, 130);
    visuals.selection.stroke = egui::Stroke::new(1.0, Color32::WHITE);

    ctx.set_visuals(visuals);
    // 紧凑间距：悬挂角落小工具，卡片内垂直间距收紧
    ctx.style_mut(|s| s.spacing.item_spacing.y = 6.0);
}

/// 海克斯风格复选框：选中时边框与勾号为亮金色（#C8AA6E），圆角硬朗，悬停仅亮度提升。
pub(super) fn hex_checkbox(
    ui: &mut egui::Ui,
    checked: &mut bool,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    let icon_width = ui.spacing().icon_width;
    let icon_spacing = ui.spacing().icon_spacing;
    let text = text.into();
    let wrap_width = ui.available_width() - (icon_width + icon_spacing);
    let galley = text.into_galley(ui, None, wrap_width, egui::TextStyle::Button);
    let mut desired = egui::vec2(icon_width + icon_spacing + galley.size().x, galley.size().y);
    desired.y = desired.y.max(icon_width).max(ui.spacing().interact_size.y);
    let (rect, mut resp) = ui.allocate_exact_size(desired, egui::Sense::click());
    if resp.clicked() {
        *checked = !*checked;
        resp.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let (small, big) = ui.spacing().icon_rectangles(rect);
        let enabled = ui.is_enabled();
        let hovered = resp.hovered();
        let gold = Color32::from_rgb(200, 170, 110);
        let mut border = if *checked { gold } else { Color32::from_rgb(72, 88, 104) };
        let mut fill = Color32::from_rgb(16, 26, 38);
        if hovered {
            border = border.gamma_multiply(1.3);
            fill = fill.gamma_multiply(1.3);
        }
        if !enabled {
            border = border.gamma_multiply(0.45);
            fill = fill.gamma_multiply(0.45);
        }
        ui.painter().rect(big, 3.0, fill, egui::Stroke::new(1.0, border));
        if *checked {
            let mark = if enabled { gold } else { gold.gamma_multiply(0.45) };
            ui.painter().add(egui::Shape::line(
                vec![
                    egui::pos2(small.left(), small.center().y),
                    egui::pos2(small.center().x, small.bottom()),
                    egui::pos2(small.right(), small.top()),
                ],
                egui::Stroke::new(1.6, mark),
            ));
        }
        let text_color = if !enabled {
            Color32::from_gray(110)
        } else if hovered {
            Color32::WHITE
        } else {
            Color32::from_rgb(222, 226, 230)
        };
        ui.painter().galley(
            egui::pos2(
                rect.min.x + icon_width + icon_spacing,
                rect.center().y - 0.5 * galley.size().y,
            ),
            galley,
            text_color,
        );
    }
    resp
}

/// 状态胶囊按钮：底色表达状态，悬停仅亮度提升（不染主色），按下略压暗。
pub(super) fn state_pill(
    ui: &mut egui::Ui,
    text: egui::RichText,
    base: Color32,
    min_size: egui::Vec2,
) -> egui::Response {
    let saved = ui.visuals().widgets.clone();
    {
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.bg_fill = base;
        w.inactive.weak_bg_fill = base;
        w.hovered.bg_fill = base.gamma_multiply(1.28);
        w.hovered.weak_bg_fill = base.gamma_multiply(1.28);
        w.hovered.fg_stroke = egui::Stroke::new(1.0, Color32::WHITE);
        w.active.bg_fill = base.gamma_multiply(0.82);
        w.active.weak_bg_fill = base.gamma_multiply(0.82);
        w.active.fg_stroke = egui::Stroke::new(1.0, Color32::WHITE);
    }
    let resp = ui.add(
        egui::Button::new(text)
            .min_size(min_size)
            .rounding(min_size.y * 0.5)
            .stroke(egui::Stroke::new(1.0, base.gamma_multiply(1.5))),
    );
    ui.visuals_mut().widgets = saved;
    resp
}

/// 1px 暗金分割线，用于功能分组（自动化流程 / 社交伪装）。
pub(super) fn gold_divider(ui: &mut egui::Ui) {
    ui.add_space(3.0);
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(96, 74, 38));
    ui.add_space(3.0);
}

/// 标题栏窗口按钮：统一 22x22 微圆角热区，平时透明，hover 高亮指定色。
pub(super) fn title_button(ui: &mut egui::Ui, glyph: &str, fg: Color32, hover: Color32) -> egui::Response {
    let saved = ui.visuals().widgets.clone();
    {
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.bg_fill = Color32::TRANSPARENT;
        w.inactive.weak_bg_fill = Color32::TRANSPARENT;
        w.inactive.bg_stroke = egui::Stroke::NONE;
        w.inactive.fg_stroke = egui::Stroke::new(1.0, fg);
        w.inactive.rounding = egui::Rounding::same(3.0);
        w.hovered.bg_fill = hover;
        w.hovered.weak_bg_fill = hover;
        w.hovered.bg_stroke = egui::Stroke::NONE;
        w.hovered.fg_stroke = egui::Stroke::new(1.0, Color32::WHITE);
        w.hovered.rounding = egui::Rounding::same(3.0);
        w.active.bg_fill = hover.gamma_multiply(0.8);
        w.active.weak_bg_fill = hover.gamma_multiply(0.8);
        w.active.bg_stroke = egui::Stroke::NONE;
        w.active.fg_stroke = egui::Stroke::new(1.0, Color32::WHITE);
        w.active.rounding = egui::Rounding::same(3.0);
    }
    let resp = ui.add(egui::Button::new(glyph).min_size(egui::vec2(22.0, 22.0)));
    ui.visuals_mut().widgets = saved;
    resp
}

/// 倒计时输入：未启用锁定时文字变灰，但仍可修改。
pub(super) fn countdown(ui: &mut egui::Ui, on: bool, val: &mut u64) -> bool {
    let saved = ui.visuals().widgets.clone();
    let c = if on {
        Color32::from_gray(210)
    } else {
        Color32::from_gray(110)
    };
    {
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.fg_stroke = egui::Stroke::new(1.0, c);
        w.hovered.fg_stroke = egui::Stroke::new(1.0, c);
        w.active.fg_stroke = egui::Stroke::new(1.0, c);
    }
    ui.label(egui::RichText::new("倒计时:").color(c));
    let changed = ui
        .add(egui::DragValue::new(val).range(1..=30).suffix("s"))
        .changed();
    ui.visuals_mut().widgets = saved;
    changed
}
