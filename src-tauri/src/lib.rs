mod api;
mod auth;
mod commands;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WebviewWindow,
};

const TRAY_ID: &str = "main-tray";
const HIDE_DELAY: Duration = Duration::from_secs(3);
/// Popup width in logical pixels. Must match the `width` in tauri.conf.json
/// and the value used by `fit_height`, otherwise the window visibly resizes
/// each time the tray is clicked.
const POPUP_WIDTH: f64 = 340.0;

/// Flag to suppress auto-hide when the frontend intentionally keeps the window open
/// (e.g. while the login form is active).
static ALLOW_AUTO_HIDE: AtomicBool = AtomicBool::new(true);
/// Set on focus-loss, cleared on focus-gain — prevents a delayed hide if the
/// window regains focus before the 3 s timer fires.
static PENDING_HIDE: AtomicBool = AtomicBool::new(false);

/// Show the popup window anchored just below the tray icon on macOS.
/// The webview is 340 wide; center it under the click x.
fn show_popup_at(window: &WebviewWindow, x: f64, y: f64) {
    let w = POPUP_WIDTH;
    // Reuse the current height if the frontend has already fitted it to the
    // content; otherwise fall back to the default. We avoid forcing 510 every
    // time, which would override the height the frontend measured.
    let h = window
        .outer_size()
        .ok()
        .map(|s| s.to_logical(window.scale_factor().unwrap_or(1.0)).height)
        .filter(|h| *h > 1.0)
        .unwrap_or(510.0);
    let _ = window.set_position(tauri::LogicalPosition::new(x - w / 2.0, y));
    let _ = window.set_size(tauri::LogicalSize::new(w, h));
    let _ = window.show();
    let _ = window.set_focus();
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    // Do NOT attach a menu to the tray icon — on macOS a menu intercepts
    // left-click and prevents on_tray_icon_event from firing, which means
    // the popup window can never appear.  Quit is handled via the UI instead.
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().cloned().expect("missing icon"))
        .tooltip("Token Monitor")
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                position,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(win) = app.get_webview_window("main") {
                    if win.is_visible().unwrap_or(false) {
                        let _ = win.hide();
                    } else {
                        let sf = win.scale_factor().unwrap_or(1.0);
                        let lx = position.x / sf;
                        let ly = position.y / sf;
                        show_popup_at(&win, lx, ly);
                    }
                }
            }
        });

    // Start with a placeholder title; the frontend updates it with today's cost.
    builder = builder.title("");
    let _tray = builder.build(app)?;
    Ok(())
}

/// Allow the frontend to control whether the window auto-hides on focus loss.
/// The login form needs the window to stay open even when focus briefly leaves.
#[tauri::command]
fn set_auto_hide(enabled: bool) {
    ALLOW_AUTO_HIDE.store(enabled, Ordering::SeqCst);
}

/// Quit the application from the UI (no tray menu).
#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

/// Update the tray title (shown next to the icon in the menu bar).
#[tauri::command]
fn set_tray_title(app: tauri::AppHandle, title: String) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_title(Some(&title));
    }
}

/// Resize the popup height to match the rendered content. Width stays fixed.
#[tauri::command]
fn fit_height(app: tauri::AppHandle, height: f64) {
    if let Some(win) = app.get_webview_window("main") {
        let sf = win.scale_factor().unwrap_or(1.0);
        let _ = win.set_size(tauri::LogicalSize::new(POPUP_WIDTH, height / sf));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::login,
            commands::login_with_token,
            commands::logout,
            commands::is_logged_in,
            commands::fetch_stats,
            commands::fetch_subscriptions,
            commands::fetch_user_info,
            set_auto_hide,
            quit_app,
            set_tray_title,
            fit_height,
        ])
        .setup(|app| {
            // Dock icon is hidden via LSUIElement=true in the bundle's Info.plist
            // (configured through the bundle config / build hooks), so a pure
            // menu-bar accessory is achieved without extra runtime crates.
            build_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Focused(false) => {
                // Delay hiding by 3 s so the user has time to interact with
                // the login form.  If the window regains focus before the
                // timer fires the pending hide is cancelled.
                PENDING_HIDE.store(true, Ordering::SeqCst);
                let win = window.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(HIDE_DELAY);
                    if PENDING_HIDE.load(Ordering::SeqCst)
                        && ALLOW_AUTO_HIDE.load(Ordering::SeqCst)
                    {
                        let _ = win.hide();
                    }
                });
            }
            tauri::WindowEvent::Focused(true) => {
                // Window came back — cancel any pending delayed hide.
                PENDING_HIDE.store(false, Ordering::SeqCst);
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
