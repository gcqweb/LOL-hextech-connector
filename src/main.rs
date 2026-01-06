#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;
mod lcu;
mod models;
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
    let (tx_gui, rx_gui) = crossbeam_channel::unbounded();
    let (tx_backend, rx_backend) = crossbeam_channel::unbounded();
    let shared_heroes = Arc::new(Mutex::new(HashMap::new()));
    let heroes_clone = shared_heroes.clone();

    tokio::spawn(async move {
        run_backend(tx_gui, rx_backend, heroes_clone).await;
    });

    let options = NativeOptions {
        // 强制指定渲染器为 Glow (OpenGL)
        // 这通常能显著降低程序启动时的内存占用 (从 ~200MB 降至 ~40MB)
        renderer: Renderer::Glow,
        // multisampling: 0, // 如果透明失效，尝试添加这一行
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([320.0, 680.0])
            .with_decorations(false) // 无边框
            .with_transparent(true), // 背景透明
        ..Default::default()
    };

    eframe::run_native(
        "海克斯 • 连接器",
        options,
        Box::new(|cc| Ok(Box::new(HexApp::new(cc, tx_backend, rx_gui, shared_heroes)))),
    )
    .unwrap();
    Ok(())
}
