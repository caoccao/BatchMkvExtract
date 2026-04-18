/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.

 *   Licensed under the Apache License, Version 2.0 (the "License");
 *   you may not use this file except in compliance with the License.
 *   You may obtain a copy of the License at

 *   http://www.apache.org/licenses/LICENSE-2.0

 *   Unless required by applicable law or agreed to in writing, software
 *   distributed under the License is distributed on an "AS IS" BASIS,
 *   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 *   See the License for the specific language governing permissions and
 *   limitations under the License.
 */

mod config;
mod constants;
mod controller;
mod mkvtoolnix;
mod protocol;

use std::sync::atomic::{AtomicBool, Ordering};

static WINDOW_READY: AtomicBool = AtomicBool::new(false);

fn convert_error(error: anyhow::Error) -> String {
    error.to_string()
}

#[tauri::command]
async fn get_about() -> Result<protocol::About, String> {
    controller::get_about().await.map_err(convert_error)
}

#[tauri::command]
async fn get_config() -> Result<config::Config, String> {
    controller::get_config().await.map_err(convert_error)
}

#[tauri::command]
async fn get_mkv_files(paths: Vec<String>) -> Result<Vec<String>, String> {
    controller::get_mkv_files(paths).await.map_err(convert_error)
}

#[tauri::command]
async fn get_mkv_tracks(file: String) -> Result<Vec<protocol::MkvTrack>, String> {
    mkvtoolnix::get_mkv_tracks(file)
        .await
        .map_err(convert_error)
}

#[tauri::command]
async fn is_mkvextract_found(path: String) -> Result<protocol::MkvextractStatus, String> {
    mkvtoolnix::is_mkvextract_found(path)
        .await
        .map_err(convert_error)
}

#[tauri::command]
async fn run_mkvextract(file: String, args: Vec<String>) -> Result<(), String> {
    mkvtoolnix::run_mkvextract(file, args)
        .await
        .map_err(convert_error)
}

#[tauri::command]
async fn set_config(config: config::Config) -> Result<config::Config, String> {
    controller::set_config(config).await.map_err(convert_error)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("Failed to build Tokio runtime");
    tauri::async_runtime::set(runtime.handle().clone());

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_about,
            get_config,
            get_mkv_files,
            get_mkv_tracks,
            is_mkvextract_found,
            run_mkvextract,
            set_config
        ])
        .setup(|app| {
            use tauri::Manager;
            let window = app.get_webview_window("main").unwrap();
            window.set_title(&format!("BatchMkvExtract v{}", env!("CARGO_PKG_VERSION")))?;

            let cfg = config::get_config();
            let _ = window.set_size(tauri::LogicalSize::new(
                cfg.window.size.width,
                cfg.window.size.height,
            ));
            if cfg.window.position.x < 0 || cfg.window.position.y < 0 {
                let _ = window.center();
            } else {
                let _ = window.set_position(tauri::LogicalPosition::new(
                    cfg.window.position.x,
                    cfg.window.position.y,
                ));
            }

            let _ = window.show();
            let _ = window.set_focus();

            WINDOW_READY.store(true, Ordering::SeqCst);
            Ok(())
        })
        .on_window_event(|window, event| {
            if !WINDOW_READY.load(Ordering::SeqCst) || window.label() != "main" {
                return;
            }
            match event {
                tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                    let Ok(scale) = window.scale_factor() else { return; };
                    let Ok(pos) = window.outer_position() else { return; };
                    let Ok(size) = window.inner_size() else { return; };
                    let logical_pos: tauri::LogicalPosition<i32> = pos.to_logical(scale);
                    let logical_size: tauri::LogicalSize<u32> = size.to_logical(scale);
                    let mut cfg = config::get_config();
                    cfg.window.position.x = logical_pos.x;
                    cfg.window.position.y = logical_pos.y;
                    cfg.window.size.width = logical_size.width;
                    cfg.window.size.height = logical_size.height;
                    if let Err(err) = config::set_config(cfg) {
                        log::error!("Couldn't save window state because {}", err);
                    }
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
