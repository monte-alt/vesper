//! Shared runtime-tunable controls updated by the Vesper tilde (`~`) menu and read
//! lock-free by both `vesper-gpu` (present tearing, HUD/menu gating) and
//! `vesper-hle` (KBM stand-in toggle, mouse sensitivity, Y-axis invert).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};

fn parse_flag(s: &str) -> bool {
    matches!(s.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

static MENU_OPEN: AtomicBool = AtomicBool::new(false);
static TEARING: OnceLock<AtomicBool> = OnceLock::new();
static KBM: OnceLock<AtomicBool> = OnceLock::new();
static MOUSE_SENS: OnceLock<AtomicU32> = OnceLock::new();
static MOUSE_INVERT_Y: OnceLock<AtomicBool> = OnceLock::new();

/// Whether the interactive Vesper control menu (`~`) is currently open.
pub fn menu_open() -> bool {
    MENU_OPEN.load(Relaxed)
}

/// Opens or closes the interactive Vesper control menu.
pub fn set_menu_open(open: bool) {
    MENU_OPEN.store(open, Relaxed);
}

fn tearing_cell() -> &'static AtomicBool {
    TEARING.get_or_init(|| {
        let on = std::env::var_os("VESPER_TEARING").is_none_or(|v| v != "0");
        AtomicBool::new(on)
    })
}

/// Whether `DXGI_PRESENT_ALLOW_TEARING` is enabled at Present time (when supported).
pub fn tearing_enabled() -> bool {
    tearing_cell().load(Relaxed)
}

pub fn set_tearing_enabled(on: bool) {
    tearing_cell().store(on, Relaxed);
}

fn kbm_cell() -> &'static AtomicBool {
    KBM.get_or_init(|| {
        AtomicBool::new(std::env::var("VESPER_KBM_PC").is_ok_and(|v| parse_flag(&v)))
    })
}

/// Whether PC keyboard + mouse look (`VESPER_KBM_PC`) is currently enabled.
pub fn kbm_enabled() -> bool {
    kbm_cell().load(Relaxed)
}

pub fn set_kbm_enabled(on: bool) {
    kbm_cell().store(on, Relaxed);
}

fn default_sens() -> f32 {
    std::env::var("VESPER_MOUSE_SENS")
        .ok()
        .and_then(|s| s.trim().parse::<f32>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(1.0)
        .clamp(0.1, 5.0)
}

fn sens_cell() -> &'static AtomicU32 {
    MOUSE_SENS.get_or_init(|| AtomicU32::new(default_sens().to_bits()))
}

/// Current mouse-look sensitivity multiplier.
pub fn mouse_sens() -> f32 {
    f32::from_bits(sens_cell().load(Relaxed)).clamp(0.1, 5.0)
}

pub fn set_mouse_sens(sens: f32) {
    let clamped = if sens.is_finite() { sens.clamp(0.1, 5.0) } else { 1.0 };
    sens_cell().store(clamped.to_bits(), Relaxed);
}

fn invert_y_cell() -> &'static AtomicBool {
    MOUSE_INVERT_Y.get_or_init(|| {
        AtomicBool::new(std::env::var("VESPER_MOUSE_INVERT_Y").is_ok_and(|v| parse_flag(&v)))
    })
}

/// Whether mouse Y axis look is inverted.
pub fn mouse_invert_y() -> bool {
    invert_y_cell().load(Relaxed)
}

pub fn set_mouse_invert_y(invert: bool) {
    invert_y_cell().store(invert, Relaxed);
}

/// Resets input & present live controls back to startup defaults.
pub fn reset_defaults() {
    let tearing = std::env::var_os("VESPER_TEARING").is_none_or(|v| v != "0");
    set_tearing_enabled(tearing);
    set_kbm_enabled(std::env::var("VESPER_KBM_PC").is_ok_and(|v| parse_flag(&v)));
    set_mouse_sens(default_sens());
    set_mouse_invert_y(std::env::var("VESPER_MOUSE_INVERT_Y").is_ok_and(|v| parse_flag(&v)));
}
