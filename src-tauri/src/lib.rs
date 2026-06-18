mod api;
mod auth;
mod commands;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, Rect, WebviewWindow,
};

#[cfg(target_os = "macos")]
use tauri::ActivationPolicy;

const TRAY_ID: &str = "main-tray";
const HIDE_DELAY: Duration = Duration::from_millis(200);
const OUTSIDE_HIDE_DELAY: Duration = Duration::from_millis(500);
const OUTSIDE_POLL_INTERVAL: Duration = Duration::from_millis(100);
/// Popup width in logical pixels. Must match the `width` in tauri.conf.json
/// and the value used by `fit_height`, otherwise the window visibly resizes
/// each time the tray is clicked.
const POPUP_WIDTH: f64 = 340.0;

/// Flag to suppress auto-hide when the frontend intentionally keeps the window open
/// (e.g. while the login form is active).
static ALLOW_AUTO_HIDE: AtomicBool = AtomicBool::new(true);
/// Set on focus-loss, cleared on focus-gain to prevent a delayed hide if the
/// window regains focus before the short timer fires.
static PENDING_HIDE: AtomicBool = AtomicBool::new(false);
static POPUP_WATCH_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
struct PhysicalBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl PhysicalBounds {
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.x + self.width && y >= self.y && y <= self.y + self.height
    }

    fn padded(self, padding: f64) -> Self {
        Self {
            x: self.x - padding,
            y: self.y - padding,
            width: self.width + padding * 2.0,
            height: self.height + padding * 2.0,
        }
    }
}

fn rect_to_physical_bounds(rect: Rect, scale_factor: f64) -> PhysicalBounds {
    let position = rect.position.to_physical::<f64>(scale_factor);
    let size = rect.size.to_physical::<f64>(scale_factor);

    PhysicalBounds {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    }
}

fn popup_position(
    window: &WebviewWindow,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> tauri::PhysicalPosition<i32> {
    let Some(monitor) = window
        .available_monitors()
        .ok()
        .and_then(|monitors| {
            monitors.into_iter().find(|monitor| {
                let area = monitor.work_area();
                x >= f64::from(area.position.x)
                    && x <= f64::from(area.position.x) + f64::from(area.size.width)
                    && y >= f64::from(area.position.y)
                    && y <= f64::from(area.position.y) + f64::from(area.size.height)
            })
        })
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten())
    else {
        let sf = window.scale_factor().unwrap_or(1.0);
        return tauri::PhysicalPosition::new((x - w * sf / 2.0).round() as i32, y.round() as i32);
    };

    let work_area = monitor.work_area();
    let sf = monitor.scale_factor();
    let gap = 8.0 * sf;
    let w = w * sf;
    let h = h * sf;
    let min_x = f64::from(work_area.position.x);
    let min_y = f64::from(work_area.position.y);
    let width = f64::from(work_area.size.width);
    let height = f64::from(work_area.size.height);
    let max_x = min_x + width;
    let max_y = min_y + height;

    let mut popup_x = x - w / 2.0;
    popup_x = if width > w {
        popup_x.clamp(min_x, max_x - w)
    } else {
        min_x
    };

    let mut popup_y = y + gap;
    if popup_y + h > max_y {
        popup_y = y - h - gap;
    }
    popup_y = if height > h {
        popup_y.clamp(min_y, max_y - h)
    } else {
        min_y
    };

    tauri::PhysicalPosition::new(popup_x.round() as i32, popup_y.round() as i32)
}

/// Show the popup window anchored near the tray icon, keeping it inside the
/// current monitor's work area.
fn show_popup_at(window: &WebviewWindow, x: f64, y: f64, tray_rect: Rect) {
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
    let _ = window.set_position(popup_position(window, x, y, w, h));
    let _ = window.set_size(tauri::LogicalSize::new(w, h));
    let _ = window.show();
    let _ = window.set_focus();
    let tray_bounds = rect_to_physical_bounds(tray_rect, window.scale_factor().unwrap_or(1.0));
    start_outside_hide_watch(window.clone(), tray_bounds);
}

fn start_outside_hide_watch(window: WebviewWindow, tray_bounds: PhysicalBounds) {
    let watch_id = POPUP_WATCH_ID.fetch_add(1, Ordering::SeqCst) + 1;

    std::thread::spawn(move || {
        let mut outside_for = Duration::ZERO;
        let tray_bounds = tray_bounds.padded(16.0);

        loop {
            std::thread::sleep(OUTSIDE_POLL_INTERVAL);

            if POPUP_WATCH_ID.load(Ordering::SeqCst) != watch_id {
                return;
            }
            if !window.is_visible().unwrap_or(false) {
                return;
            }
            if !ALLOW_AUTO_HIDE.load(Ordering::SeqCst) {
                outside_for = Duration::ZERO;
                continue;
            }
            if window.is_focused().unwrap_or(false) {
                outside_for = Duration::ZERO;
                continue;
            }

            let Ok(cursor) = window.cursor_position() else {
                continue;
            };
            let Ok(position) = window.outer_position() else {
                continue;
            };
            let Ok(size) = window.outer_size() else {
                continue;
            };

            let inside_window = cursor.x >= f64::from(position.x)
                && cursor.x <= f64::from(position.x) + f64::from(size.width)
                && cursor.y >= f64::from(position.y)
                && cursor.y <= f64::from(position.y) + f64::from(size.height);

            if inside_window || tray_bounds.contains(cursor.x, cursor.y) {
                outside_for = Duration::ZERO;
            } else {
                outside_for += OUTSIDE_POLL_INTERVAL;
                if outside_for >= OUTSIDE_HIDE_DELAY {
                    let _ = window.hide();
                    return;
                }
            }
        }
    });
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    // On macOS: do NOT attach a menu to the tray icon, otherwise a menu
    // intercepts left-click and prevents on_tray_icon_event from firing.
    // On Windows: menus work fine, but we handle quit from the UI, so the
    // tray-icon-left-click → popup-toggling pattern is consistent across both.
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().cloned().expect("missing icon"))
        .tooltip("Token Monitor | 未登录")
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                position,
                rect,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(win) = app.get_webview_window("main") {
                    if win.is_visible().unwrap_or(false) {
                        let _ = win.hide();
                    } else {
                        show_popup_at(&win, position.x, position.y, rect);
                    }
                }
            }
        });

    // Start with a placeholder title; the frontend updates it with today's cost.
    // On macOS this shows text next to the tray icon in the menu bar.
    // On Windows the tray icon cannot display text, so the cost goes to the tooltip.
    builder = builder.title("");
    let _tray = builder.build(app)?;
    Ok(())
}

// ── Tray title helpers ──────────────────────────────────────────────────────
// macOS: tray icons can display adjacent text (set_title).
// Windows: no text on tray icons — we repurpose the tooltip to show the cost.

#[cfg(target_os = "macos")]
fn set_tray_title_impl(tray: &tauri::tray::TrayIcon, title: &str) {
    let _ = tray.set_title(Some(title));
}

#[cfg(not(target_os = "macos"))]
fn set_tray_title_impl(tray: &tauri::tray::TrayIcon, title: &str) {
    let tooltip = format!("Token Monitor | {}", title);
    let _ = tray.set_tooltip(Some(&tooltip));
}

// ── Tauri commands ──────────────────────────────────────────────────────────

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

/// Update the tray title (macOS: text next to icon; Windows: tooltip).
#[tauri::command]
fn set_tray_title(app: tauri::AppHandle, title: String) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        set_tray_title_impl(&tray, &title);
    }
}

/// Resize the popup height to match the rendered content. Width stays fixed.
#[tauri::command]
fn fit_height(app: tauri::AppHandle, height: f64) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.set_size(tauri::LogicalSize::new(POPUP_WIDTH, height));

        if let (Ok(position), Ok(Some(monitor))) = (win.outer_position(), win.current_monitor()) {
            let sf = monitor.scale_factor();
            let work_area = monitor.work_area();
            let h = height * sf;
            let min_y = f64::from(work_area.position.y);
            let max_y = min_y + f64::from(work_area.size.height);
            let y = f64::from(position.y);

            if y + h > max_y {
                let adjusted_y = if f64::from(work_area.size.height) > h {
                    max_y - h
                } else {
                    min_y
                };
                let _ = win.set_position(tauri::PhysicalPosition::new(
                    position.x,
                    adjusted_y.round() as i32,
                ));
            }
        }
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
            // Hide Dock icon on macOS (accessory = menu-bar only, no Dock/App
            // Switcher). LSUIElement=true in Info.plist handles the bundled
            // build; this call ensures dev mode behaves the same. No-op on
            // Windows where the concept doesn't exist.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(ActivationPolicy::Accessory);

            build_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Focused(false) => {
                // Give focus a brief chance to settle. If the window regains
                // focus before the timer fires the pending hide is cancelled.
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
