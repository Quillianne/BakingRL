use active_win_pos_rs::get_active_window;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tracing::info;

use crate::plugin_host::PluginHost;

pub fn start_window_visibility_watcher(app_handle: AppHandle, plugin_host: Arc<PluginHost>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(1000));
        let mut is_overlay_forced_visible = false;
        let mut overlay_visible: Option<bool> = None;

        loop {
            interval.tick().await;

            let Some(overlay_window) = app_handle.get_webview_window("overlay-ingame") else {
                continue;
            };

            if app_handle.get_webview_window("overlay-editor").is_some() {
                if overlay_visible != Some(false) {
                    info!("[WindowWatcher] Hiding overlay while layout editor is open");
                }
                let _ = overlay_window.hide();
                is_overlay_forced_visible = false;
                overlay_visible = Some(false);
                continue;
            }

            let hide_when_unfocused = plugin_host
                .get_app_settings()
                .overlay
                .hide_when_game_unfocused;
            if !hide_when_unfocused {
                if !is_overlay_forced_visible {
                    info!("[WindowWatcher] Showing overlay because focus hiding is disabled");
                    let _ = overlay_window.show();
                    let _ = overlay_window.set_always_on_top(true);
                    is_overlay_forced_visible = true;
                    overlay_visible = Some(true);
                }
                continue;
            }
            is_overlay_forced_visible = false;

            let is_game_focused = match get_active_window() {
                Ok(window) => {
                    let app_name = window.app_name.to_lowercase();
                    let title = window.title.to_lowercase();

                    app_name.contains("rocket league")
                        || app_name.contains("rocketleague")
                        || title.contains("rocket league")
                        || title.contains("rocketleague")
                }
                Err(_) => false,
            };

            if is_game_focused {
                if overlay_visible != Some(true) {
                    info!("[WindowWatcher] Showing overlay");
                }
                let _ = overlay_window.show();
                let _ = overlay_window.set_always_on_top(true);
                overlay_visible = Some(true);
            } else {
                if overlay_visible != Some(false) {
                    info!("[WindowWatcher] Hiding overlay");
                }
                let _ = overlay_window.hide();
                overlay_visible = Some(false);
            }
        }
    });
}
