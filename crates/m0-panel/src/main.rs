//! m0-panel: the T2.2 layer-shell top bar (XFCE-referenced chrome).
//!
//! Connects to the compositor from the environment, anchors a Top layer
//! surface to the top edge spanning the full width, reserves its exclusive
//! zone, and software-renders the launcher label plus a UTC clock into a
//! wl_shm buffer once per second.

use std::env;
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use smithay_client_toolkit::reexports::client::globals::registry_queue_init;
use wayland_client::protocol::wl_shm;
use wayland_client::Connection;

mod render;
mod state;

use state::PanelState;

/// Panel height in logical pixels; the width follows the output.
const PANEL_HEIGHT: u32 = 42;
/// Label shown on the left side of the bar.
const LABEL: &str = "M0 PANEL :: HEADLESS-CORE LAUNCHER";
/// Poll cadence of the (event-loop free) dispatch loop.
const TICK: Duration = Duration::from_millis(50);

const HELP_TEXT: &str = "m0-panel - layer-shell top bar for the M0 desktop prototype

Usage: m0-panel

Connects to the Wayland compositor from $WAYLAND_DISPLAY/$XDG_RUNTIME_DIR and
maps a Top layer surface named m0-panel: launcher label on the left, UTC clock
on the right, Xfce-blue accent along the bottom edge.
";

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn main() -> ExitCode {
    if env::args().any(|a| a == "--help" || a == "-h") {
        print!("{HELP_TEXT}");
        return ExitCode::SUCCESS;
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("m0-panel: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| {
        format!("could not connect to a Wayland compositor ({e}); verify $WAYLAND_DISPLAY/$XDG_RUNTIME_DIR")
    })?;
    let (globals, mut event_queue) = registry_queue_init::<PanelState>(&conn)
        .map_err(|e| format!("registry roundtrip failed: {e}"))?;
    let qh = event_queue.handle();

    let mut panel = PanelState::new(&globals, &qh, PANEL_HEIGHT)?;
    conn.flush()
        .map_err(|e| format!("initial flush failed: {e}"))?;

    // Block until the first configure arrives so the panel knows its width.
    event_queue
        .blocking_dispatch(&mut panel)
        .map_err(|e| format!("dispatch failed while waiting for configure: {e}"))?;

    let mut drawn_second = u64::MAX;
    while !panel.closed {
        panel.apply_pending();
        let now = unix_now();
        let due = panel.dirty
            || (now != drawn_second && panel.buffer_free.load(std::sync::atomic::Ordering::SeqCst));
        if due && panel.size.0 > 0 {
            draw(&conn, &qh, &mut panel, now)?;
            drawn_second = now;
            panel.dirty = false;
        }
        event_queue
            .dispatch_pending(&mut panel)
            .map_err(|e| format!("dispatch failed: {e}"))?;
        conn.flush().map_err(|e| format!("flush failed: {e}"))?;
        thread::sleep(TICK);
    }
    Ok(())
}

/// Render one frame into the shm pool and commit it.
fn draw(
    conn: &Connection,
    qh: &wayland_client::QueueHandle<PanelState>,
    panel: &mut PanelState,
    now: u64,
) -> Result<(), String> {
    let (w, h) = panel.size;
    let stride = w as i32 * 4;
    let needed = (w * h * 4) as usize;
    panel
        .pool
        .resize(needed)
        .map_err(|e| format!("pool resize failed: {e}"))?;

    let clock = render::clock_text(now);
    let pixels = render::paint(w, h, LABEL, &clock);

    // The pool mmap may be larger than one frame when it outgrew the current
    // size; copy exactly one frame worth of bytes.
    let n = pixels.len().min(panel.pool.len());
    panel.pool.mmap()[..n].copy_from_slice(&pixels[..n]);

    // The loop only calls draw when the previous frame was released, so the
    // single buffer can be reused directly.
    let buffer = match panel.buffer.clone() {
        Some(buffer) => buffer,
        None => {
            let buffer = panel.pool.create_buffer(
                0,
                w as i32,
                h as i32,
                stride,
                wl_shm::Format::Xrgb8888,
                panel.buffer_free.clone(),
                qh,
            );
            panel.buffer = Some(buffer.clone());
            buffer
        }
    };
    panel
        .buffer_free
        .store(false, std::sync::atomic::Ordering::SeqCst);

    panel.surface.attach(Some(&buffer), 0, 0);
    panel.surface.damage_buffer(0, 0, w as i32, h as i32);
    panel.surface.commit();
    conn.flush()
        .map_err(|e| format!("flush after commit failed: {e}"))
}
