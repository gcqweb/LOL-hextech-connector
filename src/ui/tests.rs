    use super::*;
    use egui::{Event, Key, Modifiers, Pos2, RawInput, Rect, TextureId};

    fn hero(id: i32, name: &str, title: &str, alias: &str, image: &str) -> Hero {
        Hero {
            id,
            name: name.into(),
            title: title.into(),
            alias: alias.into(),
            image_name: image.into(),
        }
    }

    struct Harness {
        app: HexApp,
        ctx: egui::Context,
        rx_backend: crossbeam_channel::Receiver<BackendMsg>,
    }

    impl Harness {
        fn new() -> Self {
            let ctx = egui::Context::default();
            let (tx_backend, rx_backend) = crossbeam_channel::unbounded();
            let (_tx_gui, rx_gui) = crossbeam_channel::unbounded();
            let mut map = HashMap::new();
            for h in [
                hero(1, "黑暗之女", "安妮", "安妮 黑暗之女 annie", "Annie.png"),
                hero(2, "疾风剑豪", "亚索", "亚索 疾风剑豪 yasuo", "Yasuo.png"),
                hero(64, "盲僧", "李青", "李青 盲僧 leesin", "LeeSin.png"),
                hero(103, "九尾妖狐", "阿狸", "阿狸 九尾妖狐 ahri", "Ahri.png"),
                hero(266, "暗裔剑魔", "亚托克斯", "亚托克斯 暗裔剑魔 aatrox", "Aatrox.png"),
            ] {
                map.insert(h.id, h);
            }
            let heroes = Arc::new(Mutex::new(map));
            let mut app = HexApp::new(&ctx, tx_backend, rx_gui, heroes);
            app.settings = Settings::default();
            // 预热头像缓存，避免单元测试走网络
            let map = app.heroes.lock().unwrap();
            for h in map.values() {
                let img = egui::ColorImage::new([1, 1], Color32::from_rgb(200, 170, 110));
                let tex = ctx.load_texture(&h.image_name, img, Default::default());
                app.image_cache.insert(h.image_name.clone(), tex);
            }
            drop(map);
            Self { app, ctx, rx_backend }
        }

        fn pass(&mut self, events: Vec<Event>) -> egui::FullOutput {
            self.pass_with_viewport(events, None, None)
        }

        /// 注入根视口的 outer_rect / monitor_size，模拟真实窗口位置与显示器尺寸
        fn pass_with_viewport(
            &mut self,
            events: Vec<Event>,
            outer: Option<Rect>,
            monitor: Option<egui::Vec2>,
        ) -> egui::FullOutput {
            // 测试用加高视口，确保所有卡片控件都被绘制（真实窗口 680 高时靠滚动访问），
            // 使行为断言不依赖滚动位置。
            let mut raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(360.0, 1600.0))),
                events,
                ..Default::default()
            };
            if outer.is_some() || monitor.is_some() {
                let info = egui::ViewportInfo {
                    outer_rect: outer,
                    monitor_size: monitor,
                    ..Default::default()
                };
                raw.viewports.insert(egui::ViewportId::ROOT, info);
            }
            let ctx = self.ctx.clone();
            let app = &mut self.app;
            ctx.run(raw, |c| app.update_ui(c))
        }

        fn click(&mut self, pos: Pos2) -> egui::FullOutput {
            self.pass(vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::default(),
                },
            ]);
            self.pass(vec![Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            }])
        }

        /// 快速连续两次点击（真实时间间隔远小于 0.3s 双击阈值）
        fn double_click(&mut self, pos: Pos2) -> egui::FullOutput {
            self.click(pos);
            self.click(pos)
        }

        fn key(&mut self, key: Key) {
            self.pass(vec![
                Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::default(),
                },
                Event::Key {
                    key,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: Modifiers::default(),
                },
            ]);
        }

        fn type_text(&mut self, s: &str) {
            self.pass(vec![Event::Text(s.to_owned())]);
        }

        fn edit_rect(&self) -> Rect {
            self.ctx
                .read_response(egui::Id::new("snipe_search_edit"))
                .expect("搜索输入框应已渲染")
                .rect
        }

        /// egui::Area 首帧为测量帧，需若干帧后才完整绘制，这里空跑几帧使其稳定
        fn settle(&mut self) -> egui::FullOutput {
            let mut out = self.pass(vec![]);
            for _ in 0..3 {
                out = self.pass(vec![]);
            }
            out
        }

        fn result_ids(&self) -> Vec<i32> {
            self.app.search_result.iter().map(|h| h.id).collect()
        }
    }

    /// 弹出层背景框 (fill 6,16,28) 的实际绘制矩形
    fn popup_bg_rect(shapes: &[egui::epaint::ClippedShape]) -> Option<Rect> {
        fn walk(s: &egui::Shape) -> Option<Rect> {
            match s {
                egui::Shape::Vec(v) => v.iter().find_map(walk),
                egui::Shape::Rect(rs) => {
                    let f = rs.fill;
                    // 浮窗淡入动画期间 alpha 可能未满 255，按 RGB 识别即可
                    if f.r() == 6 && f.g() == 16 && f.b() == 28 && f.a() > 200 {
                        Some(rs.rect)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        shapes.iter().find_map(|cs| walk(&cs.shape))
    }

    fn has_popup_bg(shapes: &[egui::epaint::ClippedShape]) -> bool {
        popup_bg_rect(shapes).is_some()
    }

    /// 所有贴图矩形（头像等），用于校验列对齐
    fn textured_rects(shapes: &[egui::epaint::ClippedShape]) -> Vec<Rect> {
        fn walk(s: &egui::Shape, out: &mut Vec<Rect>) {
            match s {
                egui::Shape::Vec(v) => {
                    for c in v {
                        walk(c, out);
                    }
                }
                egui::Shape::Rect(rs) => {
                    if rs.fill_texture_id != TextureId::default() {
                        out.push(rs.rect);
                    }
                }
                _ => {}
            }
        }
        let mut out = vec![];
        for cs in shapes {
            walk(&cs.shape, &mut out);
        }
        out
    }

    /// 峡谷胶囊背景矩形 (fill 16,26,38)
    fn capsule_rects(shapes: &[egui::epaint::ClippedShape]) -> Vec<Rect> {
        fn walk(s: &egui::Shape, out: &mut Vec<Rect>) {
            match s {
                egui::Shape::Vec(v) => {
                    for c in v {
                        walk(c, out);
                    }
                }
                egui::Shape::Rect(rs) => {
                    let f = rs.fill;
                    if f.r() == 16 && f.g() == 26 && f.b() == 38 && f.a() > 200 {
                        out.push(rs.rect);
                    }
                }
                _ => {}
            }
        }
        let mut out = vec![];
        for cs in shapes {
            walk(&cs.shape, &mut out);
        }
        out
    }

    /// 卡片外框矩形 (fill 10,25,40)
    fn card_rects(shapes: &[egui::epaint::ClippedShape]) -> Vec<Rect> {
        fn walk(s: &egui::Shape, out: &mut Vec<Rect>) {
            match s {
                egui::Shape::Vec(v) => {
                    for c in v {
                        walk(c, out);
                    }
                }
                egui::Shape::Rect(rs) => {
                    let f = rs.fill;
                    if f.r() == 10 && f.g() == 25 && f.b() == 40 && f.a() > 200 {
                        out.push(rs.rect);
                    }
                }
                _ => {}
            }
        }
        let mut out = vec![];
        for cs in shapes {
            walk(&cs.shape, &mut out);
        }
        out
    }

    /// 文本内容及其绘制矩形（用于校验标题不被遮挡）
    fn text_rects(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, Rect)> {
        fn walk(s: &egui::Shape, out: &mut Vec<(String, Rect)>) {
            match s {
                egui::Shape::Vec(v) => {
                    for c in v {
                        walk(c, out);
                    }
                }
                egui::Shape::Text(ts) => {
                    let r = Rect::from_min_size(ts.pos, ts.galley.size());
                    out.push((ts.galley.job.text.clone(), r));
                }
                _ => {}
            }
        }
        let mut out = vec![];
        for cs in shapes {
            walk(&cs.shape, &mut out);
        }
        out
    }

    /// 收集渲染出的文本 (内容, 颜色, 中心坐标) 与贴图矩形数量
    fn analyze(shapes: &[egui::epaint::ClippedShape]) -> (Vec<(String, Color32, Pos2)>, usize) {
        fn walk(s: &egui::Shape, texts: &mut Vec<(String, Color32, Pos2)>, textured: &mut usize) {
            match s {
                egui::Shape::Vec(v) => {
                    for c in v {
                        walk(c, texts, textured);
                    }
                }
                egui::Shape::Text(ts) => {
                    let color = ts.override_text_color.unwrap_or_else(|| {
                        ts.galley
                            .job
                            .sections
                            .first()
                            .map(|sec| sec.format.color)
                            .unwrap_or(ts.fallback_color)
                    });
                    let center = ts.pos + ts.galley.size() * 0.5;
                    texts.push((ts.galley.job.text.clone(), color, center));
                }
                egui::Shape::Rect(rs) => {
                    if rs.fill_texture_id != TextureId::default() {
                        *textured += 1;
                    }
                }
                _ => {}
            }
        }
        let mut texts = vec![];
        let mut textured = 0;
        for cs in shapes {
            walk(&cs.shape, &mut texts, &mut textured);
        }
        (texts, textured)
    }

    /// 取某个已渲染文本的中心坐标（用于定位按钮 / 复选框 / 状态行）
    fn find_text(out: &egui::FullOutput, s: &str) -> Option<Pos2> {
        let (texts, _) = analyze(&out.shapes);
        texts.iter().find(|(t, _, _)| t == s).map(|(_, _, p)| *p)
    }

    #[test]
    fn keyboard_open_navigate_select_and_escape() {
        let mut h = Harness::new();
        h.pass(vec![]);
        assert!(!h.app.snipe_open, "初始应收起");

        h.click(h.edit_rect().center());
        assert!(h.app.snipe_open, "点击输入框应展开下拉");
        let out = h.settle();
        assert!(has_popup_bg(&out.shapes), "下拉应实际绘制出来");
        assert_eq!(h.result_ids(), vec![1, 2, 64, 103, 266]);
        assert_eq!(h.app.snipe_highlight, 0);

        h.key(Key::ArrowDown);
        assert_eq!(h.app.snipe_highlight, 1);
        h.key(Key::ArrowDown);
        assert_eq!(h.app.snipe_highlight, 2);
        h.key(Key::ArrowUp);
        assert_eq!(h.app.snipe_highlight, 1);

        h.key(Key::Enter);
        assert_eq!(h.app.settings.snipe_list, vec![2], "Enter 应选中高亮英雄");
        h.key(Key::ArrowDown);
        h.key(Key::Enter);
        assert_eq!(h.app.settings.snipe_list, vec![2, 64], "应支持多选");
        h.key(Key::Enter);
        assert_eq!(h.app.settings.snipe_list, vec![2], "再次 Enter 应取消选中");

        h.key(Key::ArrowUp);
        h.key(Key::ArrowUp);
        assert_eq!(h.app.snipe_highlight, 0);
        h.key(Key::ArrowUp);
        assert_eq!(h.app.snipe_highlight, 0, "顶部不应越界");
        for _ in 0..10 {
            h.key(Key::ArrowDown);
        }
        assert_eq!(h.app.snipe_highlight, 4, "底部不应越界");

        h.key(Key::Escape);
        assert!(!h.app.snipe_open, "Esc 应关闭下拉");
        let out = h.settle();
        assert!(!has_popup_bg(&out.shapes), "关闭后下拉不应再绘制");

        assert!(
            h.rx_backend.try_recv().is_ok(),
            "选择变化应触发保存"
        );
    }

    #[test]
    fn typing_filters_clear_button_and_empty_state() {
        let mut h = Harness::new();
        h.pass(vec![]);
        h.click(h.edit_rect().center());

        h.type_text("亚");
        assert_eq!(h.app.search_input, "亚");
        assert_eq!(h.result_ids(), vec![2, 266], "应按名称过滤");
        assert!(h.app.snipe_open);
        assert_eq!(h.app.snipe_highlight, 0);

        h.type_text("托");
        assert_eq!(h.result_ids(), vec![266]);
        h.key(Key::Enter);
        assert_eq!(h.app.settings.snipe_list, vec![266]);

        // 清空 × 内嵌于输入框最右侧（中心约在 max.x - 11）
        let r = h.edit_rect();
        h.click(egui::pos2(r.max.x - 11.0, r.center().y));
        assert!(h.app.search_input.is_empty(), "× 应清空输入");
        assert_eq!(h.result_ids().len(), 5, "清空后应恢复全部英雄");
        assert_eq!(h.app.settings.snipe_list, vec![266], "清空输入不应移除已选");
        assert!(h.app.snipe_open);

        h.type_text("zzz");
        assert!(h.app.search_result.is_empty());
        let out = h.pass(vec![]);
        let (texts, _) = analyze(&out.shapes);
        assert!(
            texts.iter().any(|(t, _, _)| t == "无匹配英雄"),
            "无结果时应显示提示"
        );
    }

    #[test]
    fn mouse_row_toggle_and_outside_click_close() {
        let mut h = Harness::new();
        h.pass(vec![]);
        h.click(h.edit_rect().center());
        let out = h.settle();
        let pr = popup_bg_rect(&out.shapes).expect("下拉应展开");
        let row_pos =
            |i: usize| egui::pos2(pr.center().x, pr.min.y + 4.0 + 30.0 * i as f32 + 15.0);

        h.click(row_pos(2));
        assert_eq!(h.app.settings.snipe_list, vec![64], "点击行应选中");
        assert!(h.app.snipe_open, "点击行不应关闭下拉");
        h.click(row_pos(2));
        assert!(h.app.settings.snipe_list.is_empty(), "再次点击应取消");
        h.click(row_pos(0));
        assert_eq!(h.app.settings.snipe_list, vec![1]);

        h.click(egui::pos2(160.0, 10.0));
        assert!(!h.app.snipe_open, "点击外部应关闭下拉");
    }

    #[test]
    fn selected_list_two_columns_names_avatars_and_remove() {
        let mut h = Harness::new();
        h.app.settings.snipe_list = vec![103, 2, 64, 1];
        let out = h.pass(vec![]);
        let (texts, textured) = analyze(&out.shapes);
        // 本名亮白前置
        let bright = Color32::from_rgb(235, 238, 242);
        for name in ["阿狸", "亚索", "李青", "安妮"] {
            assert!(
                texts.iter().any(|(t, c, _)| t == name && *c == bright),
                "应显示亮白英雄名 {name}"
            );
        }
        // 省略称号
        assert!(
            !texts.iter().any(|(t, _, _)| t == "九尾妖狐"),
            "已选列表不应显示称号"
        );
        assert!(!texts.iter().any(|(t, _, _)| t == "疾风剑豪"));
        // 取消序号，改用拖拽手柄（点阵绘制，无文本）：4 个英雄 → 4 个手柄交互区
        for id in [103, 2, 64, 1] {
            assert!(
                h.ctx
                    .read_response(egui::Id::new(("snipe_handle", id)))
                    .is_some(),
                "英雄 {id} 应有拖拽手柄"
            );
        }
        assert!(textured >= 4, "已选列表应渲染英雄头像贴图");
        // 2 列布局：第 1、3 项同列，第 1、2 项分列（均为两字名，中心可按列对齐）
        let cx = |name: &str| texts.iter().find(|(t, _, _)| t == name).unwrap().2.x;
        assert!((cx("阿狸") - cx("李青")).abs() < 2.0, "第1、3项应同列");
        assert!((cx("阿狸") - cx("亚索")).abs() > 20.0, "第1、2项应分列");

        // × 平时半透明，点击移除对应英雄（第一项 103）
        let faint = Color32::from_rgba_unmultiplied(160, 160, 160, 70);
        let rm_center = texts
            .iter()
            .find(|(t, c, _)| t == "×" && *c == faint)
            .map(|(_, _, p)| *p)
            .expect("移除按钮应已渲染");
        h.click(rm_center);
        assert_eq!(h.app.settings.snipe_list, vec![2, 64, 1], "× 应移除第一项");
    }

    #[test]
    fn reorder_snipe_moves_item_to_target() {
        let mut h = Harness::new();
        h.app.settings.snipe_list = vec![1, 2, 64, 103];
        h.app.reorder_snipe(64, 1);
        assert_eq!(h.app.settings.snipe_list, vec![64, 1, 2, 103]);
        h.app.reorder_snipe(64, 103);
        assert_eq!(h.app.settings.snipe_list, vec![1, 2, 64, 103]);
        h.app.reorder_snipe(2, 2);
        assert_eq!(h.app.settings.snipe_list, vec![1, 2, 64, 103], "拖到自身应无变化");
        h.app.reorder_snipe(999, 1);
        assert_eq!(h.app.settings.snipe_list, vec![1, 2, 64, 103], "未知源应无变化");
    }

    #[test]
    fn drag_handle_reorders_selected_list() {
        let mut h = Harness::new();
        h.app.settings.snipe_list = vec![1, 2, 64];
        h.pass(vec![]);
        let center = |id: i32| {
            h.ctx
                .read_response(egui::Id::new(("snipe_handle", id)))
                .expect("手柄应已渲染")
                .rect
                .center()
        };
        let src = center(64); // 英雄 64（第2行第1列）
        let dst = center(1); // 英雄 1（第1行第1列）
        let press = |pos: Pos2, pressed: bool| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        };
        h.pass(vec![Event::PointerMoved(src), press(src, true)]);
        for _ in 0..3 {
            h.pass(vec![Event::PointerMoved(dst)]);
        }
        h.pass(vec![press(dst, false)]);
        assert_eq!(
            h.app.settings.snipe_list,
            vec![64, 1, 2],
            "拖拽手柄应将 64 移到 1 的位置"
        );
    }

    #[test]
    fn dropdown_rows_render_avatars_and_checkmarks() {
        let mut h = Harness::new();
        h.app.settings.snipe_list = vec![2];
        h.pass(vec![]);
        h.click(h.edit_rect().center());
        let out = h.settle();
        let (texts, textured) = analyze(&out.shapes);
        assert!(texts.iter().any(|(t, _, _)| t == "亚索"));
        assert!(texts.iter().any(|(t, _, _)| t == "疾风剑豪"), "应显示称号");
        assert!(texts.iter().any(|(t, _, _)| t == "✓"), "已选行应有勾选标记");
        assert!(textured >= 5, "下拉每行都应渲染头像贴图");
        let gold = Color32::from_rgb(220, 190, 120);
        assert!(
            texts.iter().any(|(t, c, _)| t == "亚索" && *c == gold),
            "已选英雄名应为金色"
        );
    }

    #[test]
    fn ban_champion_name_is_coral() {
        let mut h = Harness::new();
        h.app.settings.sr_ban_enable = true;
        h.app.settings.sr_bans.insert("top".into(), 266);
        let out = h.pass(vec![]);
        let (texts, _) = analyze(&out.shapes);
        let coral = Color32::from_rgb(224, 108, 117);
        let ban = texts
            .iter()
            .find(|(t, _, _)| t == "亚托克斯")
            .expect("禁用列应渲染英雄名");
        assert_eq!(ban.1, coral, "禁用英雄名称应为柔和珊瑚红");
    }

    #[test]
    fn invisible_checkbox_controls_and_persists() {
        let mut h = Harness::new();
        h.app.connected = true;
        assert!(!h.app.settings.invisible);
        let out = h.pass(vec![]);
        let center = find_text(&out, "隐身挂机").expect("隐身复选框应渲染");
        h.click(center);
        assert!(h.app.settings.invisible, "点击应启用隐身");
        let mut got_invis = false;
        let mut got_save = false;
        while let Ok(m) = h.rx_backend.try_recv() {
            match m {
                BackendMsg::SetInvisible(v) => {
                    assert!(v);
                    got_invis = true;
                }
                BackendMsg::SaveSettings(s) => {
                    assert!(s.invisible);
                    got_save = true;
                }
                _ => {}
            }
        }
        assert!(got_invis, "启用时应发送 SetInvisible(true)");
        assert!(got_save, "应持久化隐身设置");

        // 再次点击关闭：恢复在线
        let out = h.pass(vec![]);
        let center = find_text(&out, "隐身挂机").unwrap();
        h.click(center);
        assert!(!h.app.settings.invisible);
        let mut got_off = false;
        while let Ok(m) = h.rx_backend.try_recv() {
            if matches!(m, BackendMsg::SetInvisible(false)) {
                got_off = true;
            }
        }
        assert!(got_off, "关闭时应请求恢复在线");
    }

    #[test]
    fn invisible_unaffected_by_pause() {
        let mut h = Harness::new();
        h.app.connected = true;
        h.app.settings.pause_all = true;
        let out = h.pass(vec![]);
        let center = find_text(&out, "隐身挂机").expect("暂停时隐身复选框仍应可用");
        h.click(center);
        assert!(h.app.settings.invisible, "暂停接管不应禁用隐身");
        let mut got = false;
        while let Ok(m) = h.rx_backend.try_recv() {
            if matches!(m, BackendMsg::SetInvisible(true)) {
                got = true;
            }
        }
        assert!(got, "暂停时隐身仍应生效");
    }

    #[test]
    fn invisible_disconnected_saves_preference_without_command() {
        let mut h = Harness::new();
        // 默认未连接
        let out = h.pass(vec![]);
        let center = find_text(&out, "隐身挂机").expect("隐身复选框应渲染");
        h.click(center);
        assert!(h.app.settings.invisible);
        let mut got_save = false;
        let mut got_cmd = false;
        while let Ok(m) = h.rx_backend.try_recv() {
            match m {
                BackendMsg::SaveSettings(_) => got_save = true,
                BackendMsg::SetInvisible(_) => got_cmd = true,
                _ => {}
            }
        }
        assert!(got_save, "未连接时仍应保存隐身偏好");
        assert!(!got_cmd, "未连接时不应发送隐身命令");
    }

    #[test]
    fn log_gate_hidden_by_default_and_config_unlock() {
        let mut h = Harness::new();
        assert!(!h.app.settings.show_log, "日志默认应关闭");
        let out = h.pass(vec![]);
        let (texts, _) = analyze(&out.shapes);
        assert!(
            !texts.iter().any(|(t, _, _)| t == "显示日志"),
            "默认应完全隐藏日志门禁 UI"
        );
        assert!(
            !texts.iter().any(|(t, _, _)| t.contains("初始化...")),
            "默认关闭时不应渲染日志内容"
        );

        // 用户手动改配置文件 show_log=true 解锁后，门禁 UI 才出现
        h.app.settings.show_log = true;
        let out = h.pass(vec![]);
        let (texts, _) = analyze(&out.shapes);
        assert!(
            texts.iter().any(|(t, _, _)| t == "显示日志"),
            "解锁后应渲染日志门禁"
        );
        assert!(
            texts.iter().any(|(t, _, _)| t.contains("初始化...")),
            "解锁后应渲染日志内容"
        );

        // 门禁内复选框可关闭并持久化，关闭后再次隐藏
        let center = find_text(&out, "显示日志").unwrap();
        h.click(center);
        assert!(!h.app.settings.show_log, "点击应关闭日志");
        assert!(matches!(
            h.rx_backend.try_recv(),
            Ok(BackendMsg::SaveSettings(_))
        ));
        let out = h.pass(vec![]);
        let (texts, _) = analyze(&out.shapes);
        assert!(
            !texts.iter().any(|(t, _, _)| t == "显示日志"),
            "关闭后门禁应再次隐藏"
        );
    }

    #[test]
    fn pause_toggle_disables_automation_and_persists() {
        let mut h = Harness::new();
        assert!(!h.app.settings.pause_all);
        assert!(h.app.settings.auto_accept);

        // 未暂停：自动接受可切换
        let out = h.pass(vec![]);
        let accept = find_text(&out, "自动接受").expect("自动接受应渲染");
        h.click(accept);
        assert!(!h.app.settings.auto_accept, "未暂停时点击应切换自动接受");

        // 启用暂停接管并持久化（标题行胶囊：接管中 → 已暂停）
        let out = h.pass(vec![]);
        let pause = find_text(&out, "接管中").expect("主控胶囊应渲染（接管中）");
        h.click(pause);
        assert!(h.app.settings.pause_all);
        let mut saved_pause = false;
        while let Ok(m) = h.rx_backend.try_recv() {
            if let BackendMsg::SaveSettings(s) = m {
                if s.pause_all {
                    saved_pause = true;
                }
            }
        }
        assert!(saved_pause, "暂停接管应持久化保存");

        // 暂停时自动化开关禁用：点击不改变已保存值（全局功能 + 大乱斗抢人）
        let before = h.app.settings.auto_accept;
        let out = h.pass(vec![]);
        let accept = find_text(&out, "自动接受").expect("暂停时自动接受仍渲染（禁用态）");
        h.click(accept);
        assert_eq!(h.app.settings.auto_accept, before, "暂停时自动接受应禁用");

        let snipe_before = h.app.settings.aram_snipe;
        let out = h.pass(vec![]);
        let snipe = find_text(&out, "启用秒选").expect("启用秒选应渲染");
        h.click(snipe);
        assert_eq!(h.app.settings.aram_snipe, snipe_before, "暂停时秒选应禁用");

        // 关闭暂停：恢复可编辑，且保留先前保存值
        let out = h.pass(vec![]);
        let pause = find_text(&out, "已暂停").expect("暂停态胶囊应渲染（已暂停）");
        h.click(pause);
        assert!(!h.app.settings.pause_all);
        let out = h.pass(vec![]);
        let accept = find_text(&out, "自动接受").unwrap();
        h.click(accept);
        assert_ne!(h.app.settings.auto_accept, before, "恢复后自动化开关应可再次切换");
    }

    #[test]
    fn snipe_priority_badges_show_gold_indices() {
        let mut h = Harness::new();
        h.app.settings.snipe_list = vec![103, 2, 64, 1];
        let out = h.pass(vec![]);
        let (texts, _) = analyze(&out.shapes);
        let gold = Color32::from_rgb(200, 170, 110);
        for n in ["1", "2", "3", "4"] {
            assert!(
                texts.iter().any(|(t, c, _)| t == n && *c == gold),
                "已选列表应有金色优先级序号 {n}"
            );
        }
    }

    #[test]
    fn sr_capsule_opens_singleton_picker_and_selects() {
        let mut h = Harness::new();
        h.app.settings.sr_enable = true;
        let out = h.pass(vec![]);
        let cap = find_text(&out, "未设置").expect("预选胶囊应渲染占位文字");
        h.click(cap);
        assert_eq!(
            h.app.sr_picker,
            Some(("top".into(), false)),
            "点击首个胶囊应打开 top 预选选择器"
        );
        let out = h.settle();
        assert!(has_popup_bg(&out.shapes), "应实际绘制单例选择浮窗");

        h.type_text("亚");
        assert_eq!(
            h.app.picker_results.iter().map(|x| x.id).collect::<Vec<_>>(),
            vec![2, 266],
            "浮窗应按名称过滤"
        );
        h.key(Key::Enter);
        assert_eq!(h.app.settings.sr_picks.get("top"), Some(&2), "Enter 应写回预选");
        assert!(h.app.sr_picker.is_none(), "单选后浮窗应关闭");
        assert!(
            h.rx_backend.try_recv().is_ok(),
            "写回预选应触发保存"
        );
    }

    #[test]
    fn sr_picker_is_singleton_across_capsules() {
        let mut h = Harness::new();
        h.app.settings.sr_enable = true;
        h.app.settings.sr_ban_enable = true;
        let out = h.pass(vec![]);
        let cap = find_text(&out, "未设置").unwrap();
        h.click(cap);
        assert!(h.app.sr_picker.is_some());
        // 点击浮窗外部应关闭，且同一时刻只存在一个浮窗
        h.click(egui::pos2(160.0, 10.0));
        assert!(h.app.sr_picker.is_none(), "外部点击应关闭单例浮窗");
        let out = h.settle();
        assert!(!has_popup_bg(&out.shapes), "关闭后不应再绘制浮窗");
    }

    #[test]
    fn countdown_dims_when_lock_disabled() {
        let mut h = Harness::new();
        h.app.settings.auto_lock = false;
        h.app.settings.auto_ban_lock = false;
        let out = h.pass(vec![]);
        let (texts, _) = analyze(&out.shapes);
        let cd = texts
            .iter()
            .find(|(t, _, _)| t == "倒计时:")
            .expect("倒计时标签应渲染");
        assert_eq!(cd.1, Color32::from_gray(110), "锁定未启用时倒计时应变灰");

        h.app.settings.auto_lock = true;
        let out = h.pass(vec![]);
        let (texts, _) = analyze(&out.shapes);
        let cd = texts.iter().find(|(t, _, _)| t == "倒计时:").unwrap();
        assert_eq!(cd.1, Color32::from_gray(210), "锁定启用时倒计时应恢复亮度");
    }

    #[test]
    fn window_height_auto_fits_content() {
        let mut h = Harness::new();
        let out = h.pass(vec![]);
        let cmds: Vec<egui::ViewportCommand> = out
            .viewport_output
            .values()
            .flat_map(|v| v.commands.clone())
            .collect();
        assert!(
            cmds.iter()
                .any(|c| matches!(c, egui::ViewportCommand::InnerSize(_))),
            "应发送 InnerSize 自适应窗口高度"
        );
        let h1 = h.app.last_window_h;
        assert!(
            h1 > 220.0 && h1 < 1600.0,
            "目标高度应贴合内容而非占满屏幕: {h1}"
        );
        let out2 = h.pass(vec![]);
        let cmds2: Vec<egui::ViewportCommand> = out2
            .viewport_output
            .values()
            .flat_map(|v| v.commands.clone())
            .collect();
        assert!(
            !cmds2
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::InnerSize(_))),
            "内容不变时不应反复下发 resize"
        );
        assert!((h.app.last_window_h - h1).abs() <= 0.5, "高度应收敛稳定");
    }

    #[test]
    fn window_height_does_not_collapse_when_screen_tracks_window() {
        // 回归：eframe 的 screen_rect 高度等于窗口自身尺寸。若以它做 clamp，
        // 每帧都会把窗口缩小一点，最终塌缩成一团。这里模拟该反馈循环验证不会塌缩。
        let mut h = Harness::new();
        let mut win_h = 680.0_f32;
        for _ in 0..12 {
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(360.0, win_h))),
                ..Default::default()
            };
            let ctx = h.ctx.clone();
            let app = &mut h.app;
            let out = ctx.run(raw, |c| app.update_ui(c));
            for v in out.viewport_output.values() {
                for cmd in &v.commands {
                    if let egui::ViewportCommand::InnerSize(s) = cmd {
                        win_h = s.y;
                    }
                }
            }
        }
        assert!(win_h > 400.0, "窗口不应塌缩成一团: {win_h}");
        assert!(win_h < 1000.0, "窗口应贴合内容而非无限增高: {win_h}");
    }

    #[test]
    fn window_width_auto_fits_content() {
        let mut h = Harness::new();
        let out = h.pass(vec![]);
        let cmds: Vec<egui::ViewportCommand> = out
            .viewport_output
            .values()
            .flat_map(|v| v.commands.clone())
            .collect();
        assert!(
            cmds.iter()
                .any(|c| matches!(c, egui::ViewportCommand::InnerSize(_))),
            "应发送 InnerSize 自适应窗口宽度"
        );
        let w1 = h.app.last_window_w;
        assert!(
            w1 > 240.0 && w1 < 340.0,
            "宽度应从初始 320 收缩贴合内容: {w1}"
        );
        let out2 = h.pass(vec![]);
        let cmds2: Vec<egui::ViewportCommand> = out2
            .viewport_output
            .values()
            .flat_map(|v| v.commands.clone())
            .collect();
        assert!(
            !cmds2
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::InnerSize(_))),
            "内容不变时不应反复下发 resize"
        );
        assert!((h.app.last_window_w - w1).abs() <= 0.5, "宽度应收敛稳定");
    }

    #[test]
    fn window_size_does_not_spiral_when_screen_tracks_window() {
        // 回归：screen_rect 等于窗口自身尺寸。宽/高都以其做反馈时，
        // 必须收敛到内容尺寸而非逐帧塌缩。这里同时反馈两个轴验证。
        let mut h = Harness::new();
        let mut win_w = 360.0_f32;
        let mut win_h = 680.0_f32;
        let mut prev_w = win_w;
        for _ in 0..15 {
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(win_w, win_h),
                )),
                ..Default::default()
            };
            let ctx = h.ctx.clone();
            let app = &mut h.app;
            let out = ctx.run(raw, |c| app.update_ui(c));
            prev_w = win_w;
            for v in out.viewport_output.values() {
                for cmd in &v.commands {
                    if let egui::ViewportCommand::InnerSize(s) = cmd {
                        win_w = s.x;
                        win_h = s.y;
                    }
                }
            }
        }
        assert!(
            (win_w - prev_w).abs() <= 0.5,
            "宽度应收敛而非逐帧漂移: {win_w} vs {prev_w}"
        );
        assert!(
            win_w > 240.0 && win_w < 340.0,
            "宽度应贴合内容而非塌缩/保持过宽: {win_w}"
        );
        assert!(
            win_h > 400.0 && win_h < 1000.0,
            "高度应贴合内容而非塌缩/无限增高: {win_h}"
        );
    }

    #[test]
    fn sr_capsules_fit_within_content_width() {
        let mut h = Harness::new();
        let out = h.settle();
        // 胶囊宽 = (content_w300 - 标签26 - 2*间距6)/2 = 131，高 22；过滤同色小复选框
        let caps: Vec<Rect> = capsule_rects(&out.shapes)
            .into_iter()
            .filter(|r| (r.width() - 131.0).abs() < 0.6 && (r.height() - 22.0).abs() < 0.6)
            .collect();
        assert_eq!(caps.len(), 10, "应有 5 行 × 2 列共 10 个胶囊");
        // 内容左缘 = 外边距6 + 卡片内边距6 = 12；内容宽 300 → 右缘 312
        let right_limit = 6.0 + 6.0 + 300.0 + 1.0;
        for r in &caps {
            assert!(
                r.max.x <= right_limit,
                "胶囊超出内容宽度会被裁切: {:?}",
                r
            );
            assert!(r.min.x >= 6.0, "胶囊不应越过左外边距: {:?}", r);
        }
    }

    #[test]
    fn snipe_two_columns_are_regular() {
        let mut h = Harness::new();
        h.app.settings.snipe_list = vec![1, 2, 64, 103];
        let out = h.settle();
        let mut avs: Vec<Rect> = textured_rects(&out.shapes)
            .into_iter()
            .filter(|r| (r.width() - 20.0).abs() < 0.6 && (r.height() - 20.0).abs() < 0.6)
            .collect();
        assert_eq!(avs.len(), 4, "应渲染 4 个秒选头像（20×20）");
        // 按行（y）再按列（x）排序：avs[0..2] 为第一行，avs[2..4] 为第二行
        avs.sort_by(|a, b| {
            a.min
                .y
                .partial_cmp(&b.min.y)
                .unwrap()
                .then(a.min.x.partial_cmp(&b.min.x).unwrap())
        });
        // 同一行内两个头像顶部对齐（不再因单元格高低不同而垂直错位）
        assert!(
            (avs[0].min.y - avs[1].min.y).abs() < 0.6,
            "第一行两列应垂直对齐: {:?}",
            avs
        );
        assert!(
            (avs[2].min.y - avs[3].min.y).abs() < 0.6,
            "第二行两列应垂直对齐: {:?}",
            avs
        );
        // 左列两个头像 x 一致，右列两个头像 x 一致
        assert!(
            (avs[0].min.x - avs[2].min.x).abs() < 0.6,
            "左列 x 应对齐: {:?}",
            avs
        );
        assert!(
            (avs[1].min.x - avs[3].min.x).abs() < 0.6,
            "右列 x 应对齐: {:?}",
            avs
        );
        // 两列间距规整且等于 col_w + col_gap（>100 说明确为两列而非堆叠）
        let gap = avs[1].min.x - avs[0].min.x;
        assert!(gap > 100.0, "两列应有明确间距: {gap}");
        let row_gap = avs[2].min.y - avs[0].min.y;
        assert!(
            (row_gap - 29.0).abs() < 1.0,
            "行距应规整等于 row_h(26)+spacing(3): {row_gap}"
        );
    }

    #[test]
    fn all_cards_share_uniform_width() {
        let mut h = Harness::new();
        let out = h.settle();
        let cards = card_rects(&out.shapes);
        assert!(cards.len() >= 4, "应渲染至少 4 张卡片: {}", cards.len());
        let w0 = cards[0].width();
        for r in &cards {
            assert!(
                (r.width() - w0).abs() < 0.6,
                "卡片宽度应向最宽卡片对齐: {:?} vs {}",
                r,
                w0
            );
            assert!(
                (r.min.x - cards[0].min.x).abs() < 0.6,
                "卡片左缘应对齐: {:?}",
                r
            );
        }
    }

    #[test]
    fn title_full_and_not_occluded_by_controls() {
        let mut h = Harness::new();
        let out = h.settle();
        let titles: Vec<Rect> = text_rects(&out.shapes)
            .into_iter()
            .filter(|(t, _)| t == crate::utils::APP_TITLE)
            .map(|(_, r)| r)
            .collect();
        assert_eq!(titles.len(), 1, "应完整渲染应用大标题");
        let t = titles[0];
        // 标题右缘不得进入右侧控件区（right_w=150 + 间隔4）
        let app_right = 360.0 - 6.0;
        assert!(
            t.max.x <= app_right - 150.0 - 4.0 + 0.5,
            "标题被右侧控件遮挡: {:?}",
            t
        );
        assert!(t.min.x >= 6.0, "标题不应越过左外边距: {:?}", t);
    }

    #[test]
    fn window_height_exactly_fits_content_no_scroll() {
        let mut h = Harness::new();
        let out = h.settle();
        let cards = card_rects(&out.shapes);
        let content_bottom = cards.iter().map(|r| r.max.y).fold(0.0_f32, f32::max);
        // 无 ScrollArea：窗口高 = 标题栏 + 内容 min_rect + 上下外边距，
        // 内容 min_rect 底缘恰等于 app_rect 底缘，故最底卡片必完整落在窗口内（留底边距）。
        assert!(
            content_bottom + 6.0 <= h.app.last_window_h + 0.5,
            "最底卡片应完整可见（不被裁切）: bottom={} win_h={}",
            content_bottom,
            h.app.last_window_h
        );
        // 且不应留出大片空白（仅允许卡片自身底边距级别的余量）
        assert!(
            h.app.last_window_h - (content_bottom + 6.0) <= 12.0,
            "窗口高不应远超内容: win_h={} bottom={}",
            h.app.last_window_h,
            content_bottom
        );
    }

    #[test]
    fn clamp_to_screen_keeps_margin_and_recovers_offscreen() {
        // 屏幕内正常位置保持不变
        assert!((clamp_to_screen(100.0, 300.0, 1920.0, 24.0) - 100.0).abs() < 0.01);
        // 越界右侧 → 贴右边缘并留 24 边距
        assert!(
            (clamp_to_screen(5000.0, 300.0, 1920.0, 24.0) - (1920.0 - 300.0 - 24.0)).abs() < 0.01
        );
        // 越界左侧（负） → 贴左边缘并留 24 边距
        assert!((clamp_to_screen(-500.0, 300.0, 1920.0, 24.0) - 24.0).abs() < 0.01);
        // 窗口比屏幕大 → 居中
        assert!((clamp_to_screen(0.0, 2000.0, 1920.0, 24.0) - (-40.0)).abs() < 0.01);
    }

    #[test]
    fn offscreen_saved_position_restored_to_nearest_edge() {
        let mut h = Harness::new();
        h.app.settings.window_pos = Some([5000.0, 5000.0]);
        let _ = h.pass(vec![]); // 第一帧收敛尺寸
        let out = h.pass(vec![]); // 第二帧恢复位置
        let mut restored = None;
        for v in out.viewport_output.values() {
            for cmd in &v.commands {
                if let egui::ViewportCommand::OuterPosition(p) = cmd {
                    restored = Some(*p);
                }
            }
        }
        let p = restored.expect("应下发 OuterPosition 恢复窗口位置");
        let w = h.app.last_window_w;
        let hh = h.app.last_window_h;
        assert!(
            (p.x - (1920.0 - w - 24.0)).abs() < 1.0,
            "越界位置应夹回右边缘留边距: {} vs {}",
            p.x,
            1920.0 - w - 24.0
        );
        assert!(
            (p.y - (1080.0 - hh - 24.0)).abs() < 1.0,
            "越界位置应夹回下边缘留边距: {} vs {}",
            p.y,
            1080.0 - hh - 24.0
        );
    }

    #[test]
    fn window_move_persists_after_settling() {
        let mut h = Harness::new();
        let monitor = Some(egui::vec2(1920.0, 1080.0));
        let at = |x: f32, y: f32| {
            Some(Rect::from_min_size(Pos2::new(x, y), egui::vec2(324.0, 595.0)))
        };
        // 收敛尺寸并进入位置追踪（初始位置 200,150）
        h.pass_with_viewport(vec![], at(200.0, 150.0), monitor);
        h.pass_with_viewport(vec![], at(200.0, 150.0), monitor);
        // 模拟拖拽移动到新位置（原生 StartDrag 后指针可能卡在按下态，
        // 这里不注入任何指针事件，验证写盘不依赖 pointer.any_down）
        h.pass_with_viewport(vec![], at(500.0, 400.0), monitor);
        // 位置稳定超过防抖窗口后应写盘一次
        std::thread::sleep(Duration::from_millis(350));
        h.pass_with_viewport(vec![], at(500.0, 400.0), monitor);

        let mut saved = None;
        while let Ok(m) = h.rx_backend.try_recv() {
            if let BackendMsg::SaveSettings(s) = m {
                saved = s.window_pos;
            }
        }
        let p = saved.expect("移动稳定后应持久化窗口位置");
        assert!(
            (p[0] - 500.0).abs() < 1.0 && (p[1] - 400.0).abs() < 1.0,
            "应记住移动后的位置: {:?}",
            p
        );
    }

    #[test]
    fn micro_at_edge_restores_normal_fully_onscreen() {
        let mut h = Harness::new();
        h.app.settings.micro_mode = true;
        h.settle();
        let mw = h.app.last_window_w;
        let mh = h.app.last_window_h;
        let monitor = Some(egui::vec2(1920.0, 1080.0));
        let edge_x = 1920.0 - mw - 24.0;
        let at = Rect::from_min_size(Pos2::new(edge_x, 100.0), egui::vec2(mw, mh));
        h.pass_with_viewport(vec![], Some(at), monitor);
        // 小窗贴右边缘时恢复完整模式
        h.app.settings.micro_mode = false;
        h.settle();
        let p = h.app.last_window_pos.expect("应追踪窗口位置");
        assert!(
            p.x + h.app.last_window_w <= 1920.0 - 24.0 + 0.5,
            "恢复大窗后不应越界: x={} w={}",
            p.x,
            h.app.last_window_w
        );
        assert!(p.x < edge_x, "贴右边缘恢复大窗时应左移夹回");
    }

    #[test]
    fn drag_uses_manual_position_not_native_startdrag() {
        let mut h = Harness::new();
        let monitor = Some(egui::vec2(1920.0, 1080.0));
        let at = Some(Rect::from_min_size(
            Pos2::new(200.0, 150.0),
            egui::vec2(324.0, 595.0),
        ));
        h.pass_with_viewport(vec![], at, monitor);
        let p0 = egui::pos2(120.0, 22.0);
        let p1 = egui::pos2(140.0, 30.0);
        let p2 = egui::pos2(160.0, 40.0);
        h.pass_with_viewport(
            vec![
                Event::PointerMoved(p0),
                Event::PointerButton {
                    pos: p0,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::default(),
                },
            ],
            at,
            monitor,
        );
        // 第一帧移动：drag_started 记录抓取点，本帧不位移
        h.pass_with_viewport(vec![Event::PointerMoved(p1)], at, monitor);
        // 第二帧移动：窗口应随屏幕指针增量移动
        let out = h.pass_with_viewport(vec![Event::PointerMoved(p2)], at, monitor);
        let mut start_drag = false;
        let mut moved = None;
        for v in out.viewport_output.values() {
            for cmd in &v.commands {
                match cmd {
                    egui::ViewportCommand::StartDrag => start_drag = true,
                    egui::ViewportCommand::OuterPosition(p) => moved = Some(*p),
                    _ => {}
                }
            }
        }
        assert!(!start_drag, "不应使用原生 StartDrag（会触发边缘分屏）");
        let m = moved.expect("手动拖拽应下发 OuterPosition");
        assert!(
            (m.x - 220.0).abs() < 2.0 && (m.y - 160.0).abs() < 2.0,
            "窗口应随指针增量移动: {:?}",
            m
        );
        // 松手后应清除拖拽状态，不再跟随
        h.pass_with_viewport(
            vec![Event::PointerButton {
                pos: p2,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            }],
            at,
            monitor,
        );
        assert!(h.app.drag_offset.is_none(), "松手后应清除拖拽状态");
        let out = h.pass_with_viewport(vec![Event::PointerMoved(egui::pos2(300.0, 300.0))], at, monitor);
        let mut still = false;
        for v in out.viewport_output.values() {
            for cmd in &v.commands {
                if let egui::ViewportCommand::OuterPosition(_) = cmd {
                    still = true;
                }
            }
        }
        assert!(!still, "松手后移动鼠标不应再拖动窗口");
    }

    #[test]
    fn drag_follows_exactly_with_stale_outer_and_no_rest_jitter() {
        let mut h = Harness::new();
        let monitor = Some(egui::vec2(1920.0, 1080.0));
        // outer_rect 恒为旧值，模拟 Windows 移动事件滞后一帧以上
        let stale = Some(Rect::from_min_size(
            Pos2::new(200.0, 150.0),
            egui::vec2(324.0, 595.0),
        ));
        h.pass_with_viewport(vec![], stale, monitor);
        let mut actual = egui::pos2(200.0, 150.0); // 系统实际落位（整像素）
        let local = |s: egui::Pos2, a: egui::Pos2| egui::pos2(s.x - a.x, s.y - a.y);
        let outer_cmd = |out: &egui::FullOutput| {
            let mut found = None;
            for v in out.viewport_output.values() {
                for cmd in &v.commands {
                    if let egui::ViewportCommand::OuterPosition(p) = cmd {
                        found = Some(*p);
                    }
                }
            }
            found
        };
        let p0 = egui::pos2(120.0, 22.0);
        h.pass_with_viewport(
            vec![
                Event::PointerMoved(p0),
                Event::PointerButton {
                    pos: p0,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::default(),
                },
            ],
            stale,
            monitor,
        );
        // 抓取帧：记录抓取点，本帧不位移
        let s1 = egui::pos2(340.0, 180.0);
        h.pass_with_viewport(vec![Event::PointerMoved(local(s1, actual))], stale, monitor);
        // 屏幕指针 +30,+20：窗口应精确等于 屏幕指针-抓取点，不受滞后 outer 影响
        let s2 = egui::pos2(370.0, 200.0);
        let out = h.pass_with_viewport(vec![Event::PointerMoved(local(s2, actual))], stale, monitor);
        let m = outer_cmd(&out).expect("拖拽应下发 OuterPosition");
        assert!(
            (m.x - 230.0).abs() < 0.01 && (m.y - 170.0).abs() < 0.01,
            "应精确跟手到屏幕指针减抓取点: {:?}",
            m
        );
        actual = m; // 系统整像素落位
        // 静止多帧：含无新指针事件帧与 ±1px 微抖，均不应再下发位置命令（原地横跳/闪烁）
        for i in 0..6 {
            let events = if i % 3 == 2 {
                vec![]
            } else {
                let j = if i % 2 == 0 { 1.0 } else { -1.0 };
                let l = local(s2, actual);
                vec![Event::PointerMoved(egui::pos2(l.x + j, l.y))]
            };
            let out = h.pass_with_viewport(events, stale, monitor);
            assert!(outer_cmd(&out).is_none(), "静止/微抖/无事件帧不应下发位置命令");
        }
        // 继续移动：仍精确跟随
        let s3 = egui::pos2(380.0, 210.0);
        let out = h.pass_with_viewport(vec![Event::PointerMoved(local(s3, actual))], stale, monitor);
        let m = outer_cmd(&out).expect("继续拖拽应下发 OuterPosition");
        assert!(
            (m.x - 240.0).abs() < 0.01 && (m.y - 180.0).abs() < 0.01,
            "滞后 outer 下仍应精确跟手: {:?}",
            m
        );
    }

    #[test]
    fn double_click_title_enters_micro_mode_and_persists() {
        let mut h = Harness::new();
        h.settle();
        assert!(!h.app.settings.micro_mode);
        // 标题栏左侧空白处（避开右侧控件区）快速双击
        h.double_click(egui::pos2(120.0, 22.0));
        assert!(h.app.settings.micro_mode, "双击标题栏应进入微缩模式");
        let mut saved = false;
        while let Ok(m) = h.rx_backend.try_recv() {
            if let BackendMsg::SaveSettings(s) = m {
                if s.micro_mode {
                    saved = true;
                }
            }
        }
        assert!(saved, "微缩模式应持久化保存");
    }

    #[test]
    fn micro_restore_icon_exits_to_normal() {
        let mut h = Harness::new();
        h.app.settings.micro_mode = true;
        h.settle();
        let p = h
            .ctx
            .read_response(egui::Id::new(("micro_icon", 0u8)))
            .expect("微缩标题栏应有恢复窗口图标")
            .rect
            .center();
        h.click(p);
        assert!(!h.app.settings.micro_mode, "点击恢复图标应退出微缩模式");
    }

    #[test]
    fn micro_content_shows_only_four_hard_switches() {
        let mut h = Harness::new();
        h.app.settings.micro_mode = true;
        let out = h.settle();
        for name in ["接管中", "自动接受", "自动抢人", "自动匹配"] {
            assert!(find_text(&out, name).is_some(), "微缩模式应保留硬开关 {name}");
        }
        // 卡片标题与提示性文本一律不显示
        for gone in ["全局功能", "大乱斗板凳席", "大乱斗抢人", "峡谷/排位", "伪装段位", "位置"] {
            assert!(find_text(&out, gone).is_none(), "微缩模式不应显示提示文本 {gone}");
        }
        // 自动匹配不显示秒数输入框（默认 queue_delay=3）
        assert!(find_text(&out, "3").is_none(), "微缩模式不应显示秒数输入框");
        assert!(find_text(&out, "3s").is_none());
        // 顶部不显示应用名
        assert!(find_text(&out, crate::utils::APP_TITLE).is_none());
    }

    #[test]
    fn micro_title_bar_has_restore_status_min_exit_no_name() {
        let mut h = Harness::new();
        h.app.settings.micro_mode = true;
        let out = h.settle();
        assert!(find_text(&out, crate::utils::APP_TITLE).is_none(), "微缩不应显示应用名");
        for kind in [0u8, 1u8, 2u8] {
            assert!(
                h.ctx.read_response(egui::Id::new(("micro_icon", kind))).is_some(),
                "应有矢量窗口图标 {kind}"
            );
        }
        assert!(find_text(&out, "未连接").is_some(), "左侧应保留极简状态");
        assert!(find_text(&out, "↻").is_none(), "微缩不保留重连按钮");
    }

    #[test]
    fn micro_bench_shown_only_in_aram() {
        let mut h = Harness::new();
        h.app.settings.micro_mode = true;
        h.app.bench_ids = vec![1, 2];
        h.app.is_aram = false;
        let out = h.settle();
        assert_eq!(
            textured_rects(&out.shapes).len(),
            0,
            "非大乱斗不应显示板凳席头像"
        );
        h.app.is_aram = true;
        let out = h.settle();
        assert!(
            textured_rects(&out.shapes).len() >= 2,
            "大乱斗应显示板凳席头像"
        );
    }

    #[test]
    fn micro_bench_fits_five_per_row() {
        let mut h = Harness::new();
        h.app.settings.micro_mode = true;
        h.app.is_aram = true;
        h.app.bench_ids = vec![1, 2, 64, 103, 266];
        let out = h.settle();
        let rects = textured_rects(&out.shapes);
        assert_eq!(rects.len(), 5, "应渲染 5 个板凳席头像");
        let ys: Vec<f32> = rects.iter().map(|r| r.center().y).collect();
        let ymin = ys.iter().cloned().fold(f32::INFINITY, f32::min);
        let ymax = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            ymax - ymin < 1.0,
            "5 个头像应同处一行: ymin={} ymax={}",
            ymin,
            ymax
        );
    }

    #[test]
    fn micro_switches_toggle_settings() {
        let mut h = Harness::new();
        h.app.settings.micro_mode = true;
        let out = h.settle();
        assert!(h.app.settings.auto_accept);
        let p = find_text(&out, "自动接受").unwrap();
        h.click(p);
        assert!(!h.app.settings.auto_accept, "微缩硬开关应可切换自动接受");

        let out = h.pass(vec![]);
        let p = find_text(&out, "接管中").unwrap();
        h.click(p);
        assert!(h.app.settings.pause_all, "微缩硬开关应可切换暂停接管");
    }

    #[test]
    fn micro_window_is_narrower_than_normal() {
        let mut h = Harness::new();
        h.settle();
        let normal_w = h.app.last_window_w;
        h.app.settings.micro_mode = true;
        h.settle();
        assert!(
            h.app.last_window_w < normal_w - 50.0,
            "微缩窗口应明显更窄: {} vs {}",
            h.app.last_window_w,
            normal_w
        );
    }

    #[test]
    fn micro_window_height_fits_without_bench() {
        let mut h = Harness::new();
        h.app.settings.micro_mode = true;
        h.app.is_aram = false;
        let out = h.settle();
        let bottom = text_rects(&out.shapes)
            .iter()
            .filter(|(t, _)| t == "自动匹配")
            .map(|(_, r)| r.max.y)
            .fold(0.0_f32, f32::max);
        assert!(bottom > 0.0, "应渲染自动匹配复选框");
        // 板凳席隐藏时窗口不应预留大片空白：底缘仅留卡片/外边距级别余量
        assert!(
            h.app.last_window_h - bottom <= 30.0,
            "微缩窗口高不应远超内容: win_h={} bottom={}",
            h.app.last_window_h,
            bottom
        );
        assert_eq!(textured_rects(&out.shapes).len(), 0, "非大乱斗不显示板凳席");
    }

    #[test]
    fn micro_switches_two_columns_and_capsule_text_centered() {
        fn pill_rect(shapes: &[egui::epaint::ClippedShape]) -> Option<Rect> {
            fn walk(s: &egui::Shape, out: &mut Option<Rect>) {
                match s {
                    egui::Shape::Vec(v) => {
                        for c in v {
                            walk(c, out);
                        }
                    }
                    egui::Shape::Rect(rs) => {
                        let f = rs.fill;
                        if f.r() == 24 && f.g() == 66 && f.b() == 50 && f.a() > 200 {
                            *out = Some(rs.rect);
                        }
                    }
                    _ => {}
                }
            }
            let mut out = None;
            for cs in shapes {
                walk(&cs.shape, &mut out);
            }
            out
        }
        let mut h = Harness::new();
        h.app.settings.micro_mode = true;
        let out = h.settle();
        let texts = text_rects(&out.shapes);
        let r = |name: &str| {
            texts
                .iter()
                .find(|(t, _)| t == name)
                .map(|(_, r)| *r)
                .unwrap_or_else(|| panic!("应渲染 {name}"))
        };
        let pause = r("接管中");
        let accept = r("自动接受");
        let snipe = r("自动抢人");
        let queue = r("自动匹配");
        // 第一行：胶囊 | 自动接受；第二行：自动抢人 | 自动匹配
        assert!((pause.center().y - accept.center().y).abs() < 4.0, "第一行应水平对齐");
        assert!(accept.center().x > pause.center().x + 20.0, "自动接受应在右列");
        assert!((snipe.center().y - queue.center().y).abs() < 4.0, "第二行应水平对齐");
        assert!(queue.center().x > snipe.center().x + 20.0, "自动匹配应在右列");
        assert!(snipe.center().y > pause.center().y + 10.0, "应为两行而非单列");
        assert!(
            (accept.center().x - queue.center().x).abs() < 2.0,
            "右列应跨行对齐: accept_cx={} queue_cx={}",
            accept.center().x,
            queue.center().x
        );
        // 胶囊文本水平居中
        let pill = pill_rect(&out.shapes).expect("应有暂停胶囊");
        assert!(
            (pill.center().x - pause.center().x).abs() < 2.0,
            "胶囊文本应居中: pill_cx={} text_cx={}",
            pill.center().x,
            pause.center().x
        );
    }

    /// 真实下载头像验证（需要网络），手动运行：cargo test -- --ignored
    #[test]
    #[ignore]
    fn real_avatar_download_and_cache() {
        let mut h = Harness::new();
        h.app.image_cache.clear();
        let tex = h.app.get_image(&h.ctx, "Ahri.png");
        assert!(tex.is_some(), "应能下载并解码头像");
        assert!(
            Path::new(&format!("{}/Ahri.png", IMG_DIR)).exists(),
            "头像应缓存到 data/img"
        );
    }
