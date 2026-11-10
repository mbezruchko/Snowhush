//! The compositor's native surface: layer-shell elsewhere, a trusted xdg-shell window on GNOME.

use smithay_client_toolkit::compositor::{CompositorState, Region};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface};
use smithay_client_toolkit::shell::xdg::XdgShell;
use smithay_client_toolkit::shell::xdg::window::{Window, WindowDecorations};
use wayland_client::QueueHandle;
use wayland_client::protocol::{wl_output::WlOutput, wl_surface::WlSurface};

use crate::{snow::App, windows::Area};

pub enum Shell {
    Layer(LayerShell),
    Gnome(XdgShell),
}

pub enum Overlay {
    Layer(LayerSurface),
    Gnome { window: Window, name: String, layer: Layer, title: String },
}

impl Overlay {
    pub fn new(
        compositor: &CompositorState,
        shell: &Shell,
        qh: &QueueHandle<App>,
        output: &WlOutput,
        name: &str,
        bounds: Area,
        layer: Layer,
    ) -> Self {
        let surface = compositor.create_surface(qh);
        // No pointer, keyboard or scroll input is taken from the desktop underneath.
        let region = Region::new(compositor).expect("Wayland input region");
        surface.set_input_region(Some(region.wl_region()));
        let mut overlay = match shell {
            Shell::Layer(shell) => {
                let layer = shell.create_layer_surface(qh, surface, layer, Some("snowhush"), Some(output));
                layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
                layer.set_exclusive_zone(-1);
                layer.set_keyboard_interactivity(KeyboardInteractivity::None);
                Self::Layer(layer)
            }
            Shell::Gnome(shell) => {
                let window = shell.create_window(surface, WindowDecorations::None, qh);
                window.set_app_id("io.github.mbezruchko.Snowhush.Overlay");
                Self::Gnome { window, name: name.into(), layer, title: String::new() }
            }
        };
        overlay.place(bounds, (0, 0, 0, 0));
        overlay.wl_surface().commit();
        overlay
    }

    pub fn wl_surface(&self) -> &WlSurface {
        match self {
            Self::Layer(layer) => layer.wl_surface(),
            Self::Gnome { window, .. } => window.wl_surface(),
        }
    }

    pub fn place(&mut self, bounds: Area, margins: (i32, i32, i32, i32)) {
        match self {
            Self::Layer(layer) => layer.set_margin(margins.0, margins.1, margins.2, margins.3),
            Self::Gnome { window, name, layer, title } => {
                // Only the trusted Shell extension interprets this metadata. Pixels stay in the Rust/GPU process.
                let next = format!(
                    "snowhush:{}",
                    serde_json::json!({
                        "name": name,
                        "bounds": [bounds.x, bounds.y, bounds.w, bounds.h],
                        "area": [bounds.x + margins.3 as f64, bounds.y + margins.0 as f64,
                            (bounds.w - (margins.1 + margins.3) as f64).max(1.0),
                            (bounds.h - (margins.0 + margins.2) as f64).max(1.0)],
                        "behind": *layer == Layer::Bottom,
                        "over": *layer == Layer::Overlay,
                    })
                );
                if *title != next {
                    window.set_title(&next);
                    *title = next;
                }
            }
        }
        self.wl_surface().commit();
    }

    pub fn set_layer(&mut self, next: Layer, bounds: Area, margins: (i32, i32, i32, i32)) {
        match self {
            Self::Layer(layer) => layer.set_layer(next),
            Self::Gnome { layer, .. } => *layer = next,
        }
        self.place(bounds, margins);
    }
}
