//! Wayland protocol state for the panel: layer-shell surface lifecycle plus
//! the shared-memory pool that backs the rendered frames.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::shm::raw::RawPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_shm,
    registry_handlers,
};
use wayland_client::globals::GlobalList;
use wayland_client::protocol::{wl_buffer, wl_output, wl_surface};
use wayland_client::{Connection, QueueHandle};

/// Protocol-facing panel state. The main loop polls `dirty`/`closed` and
/// redraws when the clock second flips or a configure resized the bar.
#[derive(Debug)]
#[allow(
    dead_code,
    reason = "compositor/layer are kept alive for the protocol object lifetime"
)]
pub struct PanelState {
    pub registry: RegistryState,
    pub output_state: OutputState,
    pub shm: Shm,
    pub pool: RawPool,
    pub surface: wl_surface::WlSurface,
    // Held (never read) on purpose: dropping LayerSurface would send
    // wl_display.sync-destroy for the zwlr_layer_surface_v1 and unmap the bar.
    pub layer: LayerSurface,
    pub compositor: CompositorState,
    pub buffer: Option<wl_buffer::WlBuffer>,
    /// Flipped by the WlBuffer release event; read by the main loop.
    pub buffer_free: Arc<AtomicBool>,
    pub size: (u32, u32),
    pub pending_size: Option<(u32, u32)>,
    pub closed: bool,
    pub dirty: bool,
}

impl PanelState {
    /// Bind every global the panel needs and create the top layer surface.
    pub fn new(globals: &GlobalList, qh: &QueueHandle<Self>, height: u32) -> Result<Self, String> {
        let registry = RegistryState::new(globals);
        let output_state = OutputState::new(globals, qh);
        let compositor = CompositorState::bind(globals, qh)
            .map_err(|e| format!("wl_compositor unavailable: {e}"))?;
        let shm = Shm::bind(globals, qh).map_err(|e| format!("wl_shm unavailable: {e}"))?;
        let layer_shell =
            LayerShell::bind(globals, qh).map_err(|e| format!("layer-shell unavailable: {e}"))?;

        let surface = compositor.create_surface(qh);
        let layer = layer_shell.create_layer_surface(
            qh,
            surface.clone(),
            Layer::Top,
            Some("m0-panel"),
            None,
        );
        layer.set_anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT);
        layer.set_exclusive_zone(height as i32);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(0, height);
        surface.commit();
        // Start with a small pool; the main loop resizes once configured.
        // Note: sctk acks configure events itself before our handler runs.
        let pool = RawPool::new(64 * 1024, &shm).map_err(|e| format!("wl_shm pool: {e}"))?;
        Ok(Self {
            registry,
            output_state,
            compositor,
            shm,
            pool,
            surface,
            layer,
            buffer: None,
            buffer_free: Arc::new(AtomicBool::new(true)),
            size: (0, height),
            pending_size: None,
            closed: false,
            dirty: true,
        })
    }

    /// Apply a pending configure: drop the old buffer when the size changed.
    pub fn apply_pending(&mut self) {
        if let Some(size) = self.pending_size.take() {
            if size != self.size {
                if let Some(buffer) = self.buffer.take() {
                    buffer.destroy();
                }
                self.size = size;
            }
            self.dirty = true;
        }
    }
}

impl LayerShellHandler for PanelState {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.closed = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let (w, h) = configure.new_size;
        // Zero width means "you choose" for left+right anchored bars.
        let width = if w == 0 { self.size.0.max(1) } else { w };
        let height = if h == 0 { self.size.1 } else { h };
        self.pending_size = Some((width, height));
    }
}

impl CompositorHandler for PanelState {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
        // The headless harness configures 1:1; accept whatever arrives.
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
        // Transforms are likewise ignored for the M0 prototype bar.
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        // Frame callbacks are never requested; keep the trait satisfied.
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for PanelState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl ShmHandler for PanelState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

// WlBuffer release events flip the shared flag the main loop polls.
impl wayland_client::Dispatch<wl_buffer::WlBuffer, Arc<AtomicBool>> for PanelState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        data: &Arc<AtomicBool>,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            data.store(true, Ordering::SeqCst);
        }
    }
}

impl ProvidesRegistryState for PanelState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    // registry_handlers! fills in runtime_add_global / runtime_remove_global
    // by delegating to the states registered above.
    registry_handlers![OutputState];
}

delegate_registry!(PanelState);
delegate_compositor!(PanelState);
delegate_output!(PanelState);
delegate_shm!(PanelState);
delegate_layer!(PanelState);
