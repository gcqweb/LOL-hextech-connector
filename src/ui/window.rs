use super::*;


/// 将窗口某一轴坐标夹回屏幕内：越界时贴最近边缘并预留 margin；
/// 窗口比屏幕还大时居中。
pub fn clamp_to_screen(pos: f32, size: f32, screen: f32, margin: f32) -> f32 {
    let min = margin;
    let max = screen - size - margin;
    if max < min {
        (screen - size) * 0.5
    } else {
        pos.clamp(min, max)
    }
}
impl HexApp {

    /// 窗口位置持久化：启动时恢复保存的位置（越界夹回最近边缘并留边距），
    /// 移动结束后写回配置。尺寸收敛后再恢复，避免用初始尺寸夹错位置。
    pub(super) fn sync_window_position(&mut self, ctx: &egui::Context) {
        const MARGIN: f32 = 24.0;
        if !self.restored_pos {
            if self.last_window_h <= 0.0 {
                return; // 等待自适应尺寸收敛
            }
            self.restored_pos = true;
            if let Some([x, y]) = self.settings.window_pos {
                let monitor = ctx
                    .input(|i| i.viewport().monitor_size)
                    .unwrap_or(egui::vec2(1920.0, 1080.0));
                let size = egui::vec2(self.last_window_w, self.last_window_h);
                let cx = clamp_to_screen(x, size.x, monitor.x, MARGIN);
                let cy = clamp_to_screen(y, size.y, monitor.y, MARGIN);
                self.last_window_pos = Some(egui::pos2(cx, cy));
                // 连续几帧下发同一位置：egui 对根视口尺寸命令会触发额外内部重绘帧，
                // 单帧命令可能被后续重绘帧覆盖；重复下发幂等且确保生效。
                self.restore_frames = 3;
            }
        }
        if self.restore_frames > 0 {
            if let Some(p) = self.last_window_pos {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(p));
            }
            self.restore_frames -= 1;
            return; // 恢复期间不追踪 outer_rect，避免把旧位置写回配置
        }
        if self.drag_offset.is_some() {
            return; // 拖拽中位置由拖拽逻辑维护；outer_rect 滞后一帧，读它会写回旧位置
        }
        if let Some(outer) = ctx.input(|i| i.viewport().outer_rect) {
            let p = outer.min;
            let moved = self.last_window_pos.map_or(true, |lp| {
                (lp.x - p.x).abs() > 0.5 || (lp.y - p.y).abs() > 0.5
            });
            if moved {
                self.last_window_pos = Some(p);
                self.settings.window_pos = Some([p.x, p.y]);
                // 标记位置变化时刻。拖拽为手动下发 OuterPosition，逐帧移动；
                // 以位置稳定一段时间后防抖写盘，避免拖拽过程中频繁落盘。
                self.pos_dirty_since = Some(std::time::Instant::now());
                ctx.request_repaint_after(Duration::from_millis(300));
            } else if let Some(t) = self.pos_dirty_since {
                if t.elapsed() >= Duration::from_millis(300) {
                    self.trigger_save();
                    self.pos_dirty_since = None;
                } else {
                    ctx.request_repaint_after(Duration::from_millis(300));
                }
            }
        }
    }
}
