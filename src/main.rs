#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;
mod lcu;
mod models;
mod single_instance;
mod ui;
mod utils;

use anyhow::Result;
use eframe::{NativeOptions, Renderer};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use backend::run_backend;
use ui::HexApp;

#[tokio::main]
async fn main() -> Result<()> {
    // 单实例：已有实例在运行时激活其窗口并退出本进程。
    if !single_instance::ensure_single_instance(utils::APP_TITLE) {
        return Ok(());
    }

    let (tx_gui, rx_gui) = crossbeam_channel::unbounded();
    let (tx_backend, rx_backend) = crossbeam_channel::unbounded();
    let shared_heroes = Arc::new(Mutex::new(HashMap::new()));
    let heroes_clone = shared_heroes.clone();

    tokio::spawn(async move {
        run_backend(tx_gui, rx_backend, heroes_clone).await;
    });

    let mut viewport = egui::ViewportBuilder::default()
        // 显式设置窗口标题：单实例通过 FindWindowW 按此标题定位已有窗口。
        // （Windows 下 eframe 不会用 app_name 自动设置标题，缺省会变成 "egui window"）
        .with_title(utils::APP_TITLE)
        .with_inner_size([320.0, 680.0])
        .with_decorations(false) // 无边框
        .with_transparent(true); // 背景透明
    // 任务栏/alt-tab 图标跟随应用图标（否则为 eframe 默认 "e"）
    if let Some(icon) = app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = NativeOptions {
        // 强制指定渲染器为 Glow (OpenGL)
        // 这通常能显著降低程序启动时的内存占用 (从 ~200MB 降至 ~40MB)
        renderer: Renderer::Glow,
        // multisampling: 0, // 如果透明失效，尝试添加这一行
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        utils::APP_TITLE,
        options,
        Box::new(|cc| Ok(Box::new(HexApp::new(&cc.egui_ctx, tx_backend, rx_gui, shared_heroes)))),
    )
    .unwrap();
    Ok(())
}

/// 解码内嵌的应用图标为 egui 窗口图标（编译期嵌入，运行时无需外部文件）
fn app_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(include_bytes!("../icon.ico"))
        .ok()?
        .to_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn app_icon_decodes_to_valid_rgba() {
        let icon = super::app_icon().expect("icon.ico 应能解码");
        assert!(icon.width > 0 && icon.height > 0);
        assert_eq!(
            icon.rgba.len(),
            (icon.width * icon.height * 4) as usize,
            "RGBA 字节数应与尺寸匹配"
        );
    }
}
