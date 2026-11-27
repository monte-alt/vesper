//! Shared runtime-tunable controls updated by the Vesper tilde (`~`) menu and read
//! lock-free by both `vesper-gpu` (present tearing, HUD/menu gating) and
//! `vesper-hle` (KBM stand-in toggle, mouse sensitivity, Y-axis invert).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};

fn parse_flag(s: &str) -> bool {
    matches!(s.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

static MENU_OPEN: AtomicBool = AtomicBool::new(false);
static TEARING: OnceLock<AtomicBool> = OnceLock::new();
static KBM: OnceLock<AtomicBool> = OnceLock::new();
static MOUSE_SENS: OnceLock<AtomicU32> = OnceLock::new();
static MOUSE_INVERT_Y: OnceLock<AtomicBool> = OnceLock::new();
static TEX_STAGE_MS: OnceLock<AtomicU64> = OnceLock::new();
static TEX_STAGE_MB: OnceLock<AtomicU64> = OnceLock::new();
static TEX_CATCHUP: OnceLock<AtomicBool> = OnceLock::new();
static PRESSURE_FREE_MB: OnceLock<AtomicU64> = OnceLock::new();

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

fn default_tex_stage_ms() -> u64 {
    std::env::var("VESPER_TEXTURE_STAGE_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(250)
        .clamp(4, 500)
}

fn tex_stage_ms_cell() -> &'static AtomicU64 {
    TEX_STAGE_MS.get_or_init(|| AtomicU64::new(default_tex_stage_ms()))
}

/// Per-pass texture staging time cap in milliseconds (`VESPER_TEXTURE_STAGE_MS`).
pub fn tex_stage_ms() -> u64 {
    tex_stage_ms_cell().load(Relaxed).clamp(4, 500)
}

pub fn set_tex_stage_ms(ms: u64) {
    tex_stage_ms_cell().store(ms.clamp(4, 500), Relaxed);
}

fn default_tex_stage_mb() -> u64 {
    let raw = std::env::var("VESPER_TEXTURE_STAGE_MB")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(0);
    if raw == 0 { 0 } else { raw.clamp(8, 512) }
}

fn tex_stage_mb_cell() -> &'static AtomicU64 {
    TEX_STAGE_MB.get_or_init(|| AtomicU64::new(default_tex_stage_mb()))
}

/// Per-frame texture staging byte cap in MiB (`VESPER_TEXTURE_STAGE_MB`; `0` = auto).
pub fn tex_stage_mb() -> u64 {
    let v = tex_stage_mb_cell().load(Relaxed);
    if v == 0 { 0 } else { v.clamp(8, 512) }
}

pub fn set_tex_stage_mb(mb: u64) {
    tex_stage_mb_cell().store(if mb == 0 { 0 } else { mb.clamp(8, 512) }, Relaxed);
}

fn tex_catchup_cell() -> &'static AtomicBool {
    TEX_CATCHUP.get_or_init(|| {
        AtomicBool::new(std::env::var_os("VESPER_TEXTURE_CATCHUP").is_none_or(|v| v != "0"))
    })
}

/// Whether backlog `>= 1024` due slots trigger 3x staging bursts (`VESPER_TEXTURE_CATCHUP`).
pub fn tex_catchup() -> bool {
    tex_catchup_cell().load(Relaxed)
}

pub fn set_tex_catchup(on: bool) {
    tex_catchup_cell().store(on, Relaxed);
}

fn default_pressure_free_mb() -> u64 {
    std::env::var("VESPER_PRESSURE_FREE_MB")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(1536)
        .clamp(512, 8192)
}

fn pressure_free_mb_cell() -> &'static AtomicU64 {
    PRESSURE_FREE_MB.get_or_init(|| AtomicU64::new(default_pressure_free_mb()))
}

/// Free physical RAM threshold in MiB where residency pressure engages (`VESPER_PRESSURE_FREE_MB`).
pub fn pressure_free_mb() -> u64 {
    pressure_free_mb_cell().load(Relaxed).clamp(512, 8192)
}

pub fn set_pressure_free_mb(mb: u64) {
    pressure_free_mb_cell().store(mb.clamp(512, 8192), Relaxed);
}

/// Resets frame-pacing and RAM-pressure live controls back to startup defaults.
pub fn reset_pacing_defaults() {
    set_tex_stage_ms(default_tex_stage_ms());
    set_tex_stage_mb(default_tex_stage_mb());
    set_tex_catchup(std::env::var_os("VESPER_TEXTURE_CATCHUP").is_none_or(|v| v != "0"));
    set_pressure_free_mb(default_pressure_free_mb());
}

/// Resets input & present live controls back to startup defaults.
pub fn reset_defaults() {
    let tearing = std::env::var_os("VESPER_TEARING").is_none_or(|v| v != "0");
    set_tearing_enabled(tearing);
    set_kbm_enabled(std::env::var("VESPER_KBM_PC").is_ok_and(|v| parse_flag(&v)));
    set_mouse_sens(default_sens());
    set_mouse_invert_y(std::env::var("VESPER_MOUSE_INVERT_Y").is_ok_and(|v| parse_flag(&v)));
    reset_pacing_defaults();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pacing_controls_clamp_and_reset() {
        set_tex_stage_ms(1);
        assert_eq!(tex_stage_ms(), 4);
        set_tex_stage_ms(999);
        assert_eq!(tex_stage_ms(), 500);
        set_tex_stage_ms(14);
        assert_eq!(tex_stage_ms(), 14);

        set_tex_stage_mb(0);
        assert_eq!(tex_stage_mb(), 0);
        set_tex_stage_mb(2);
        assert_eq!(tex_stage_mb(), 8);
        set_tex_stage_mb(999);
        assert_eq!(tex_stage_mb(), 512);
        set_tex_stage_mb(48);
        assert_eq!(tex_stage_mb(), 48);

        set_tex_catchup(false);
        assert!(!tex_catchup());

        set_pressure_free_mb(100);
        assert_eq!(pressure_free_mb(), 512);
        set_pressure_free_mb(16384);
        assert_eq!(pressure_free_mb(), 8192);
        set_pressure_free_mb(2560);
        assert_eq!(pressure_free_mb(), 2560);

        reset_pacing_defaults();
        assert_eq!(tex_stage_ms(), default_tex_stage_ms());
        assert_eq!(tex_stage_mb(), default_tex_stage_mb());
        assert_eq!(pressure_free_mb(), default_pressure_free_mb());
    }
}
