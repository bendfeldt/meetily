//! Fork-only (bendfeldt/meetily, macOS): the "Teams meeting detected" pop-up.
//!
//! A small hidden window (`public/meeting-prompt.html`) turned into a
//! non-activating NSPanel with tauri-nspanel, so it shows above every window
//! including full-screen apps in their own Space, and clicking it neither
//! activates meetily, switches Space, nor takes keyboard focus from Teams.
//! All AppKit calls run on the main thread.

use tauri::{AppHandle, Manager, PhysicalPosition, Runtime, WebviewUrl, WebviewWindowBuilder};
use tauri_nspanel::cocoa::appkit::NSWindowCollectionBehavior;
use tauri_nspanel::{ManagerExt, WebviewWindowExt};

const LABEL: &str = "meeting-prompt";
const WIDTH: f64 = 360.0;
const HEIGHT: f64 = 110.0;
const MARGIN: f64 = 16.0;
/// NSWindowStyleMaskNonactivatingPanel: clicking does not activate the app.
const NON_ACTIVATING_PANEL: i32 = 1 << 7;
/// NSStatusWindowLevel: above full-screen app windows, below system alerts.
const STATUS_WINDOW_LEVEL: i32 = 25;

/// Create the hidden pop-up panel unless it exists. Called from the detector
/// loop the first time the feature is on, so users who never enable it get
/// no extra window. Builds the window off the main thread (like a command
/// would) and converts it to a panel on the main thread.
pub async fn ensure<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    if app.get_webview_panel(LABEL).is_ok() {
        return Ok(());
    }
    // A window left by an earlier failed attempt is converted, not rebuilt.
    let window = match app.get_webview_window(LABEL) {
        Some(window) => window,
        None => {
            WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("meeting-prompt.html".into()))
                .title("Teams meeting detected")
                .inner_size(WIDTH, HEIGHT)
                .resizable(false)
                .decorations(false)
                .transparent(true)
                .skip_taskbar(true)
                .always_on_top(true)
                .visible(false)
                .focused(false)
                .accept_first_mouse(true)
                .build()
                .map_err(|e| format!("cannot create the pop-up window: {e}"))?
        }
    };

    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(make_panel(&window));
    })
    .map_err(|e| format!("cannot reach the main thread: {e}"))?;
    rx.await
        .map_err(|_| "the main thread dropped the pop-up setup".to_string())?
        .map_err(|e| format!("cannot turn the pop-up into a panel: {e}"))
}

fn make_panel<R: Runtime>(window: &tauri::WebviewWindow<R>) -> tauri::Result<()> {
    let panel = window.to_panel()?;
    panel.set_style_mask(NON_ACTIVATING_PANEL);
    panel.set_floating_panel(true);
    panel.set_becomes_key_only_if_needed(true); // buttons work without key focus
    panel.set_hides_on_deactivate(false);
    panel.set_level(STATUS_WINDOW_LEVEL);
    panel.set_collection_behaviour(
        NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorIgnoresCycle,
    );
    Ok(())
}

/// Top-right corner of the monitor the pointer is on.
fn place<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    if let Some(monitor) = monitor {
        let scale = monitor.scale_factor();
        let pos = monitor.position();
        let size = monitor.size();
        let x = pos.x as f64 + size.width as f64 - (WIDTH + MARGIN) * scale;
        let y = pos.y as f64 + (MARGIN + 24.0) * scale; // below the menu bar
        if let Err(e) = window.set_position(PhysicalPosition::new(x, y)) {
            log::warn!("meeting_prompt: cannot place the pop-up: {e}");
        }
    }
}

pub fn show<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        place(&handle);
        match handle.get_webview_panel(LABEL) {
            Ok(panel) => panel.order_front_regardless(),
            Err(_) => log::warn!("meeting_prompt: panel missing, cannot show the pop-up"),
        }
    });
}

pub fn hide<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Ok(panel) = handle.get_webview_panel(LABEL) {
            panel.order_out(None);
        }
    });
}
