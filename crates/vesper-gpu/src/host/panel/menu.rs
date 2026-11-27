//! Interactive Tilde (`~`) Vesper Control Center menu.

use std::time::Instant;

use windows::Win32::Foundation::{HWND, POINT as WIN_POINT};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_LBUTTON, VK_OEM_3, VK_RETURN, VK_RIGHT, VK_TAB, VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use super::draw2d::Canvas;
use super::font::text_width;
use super::{
    AMBER, BORDER, CARD_BOT, CARD_TOP, CORAL, CYAN, EMERALD, GOLD, GraphCeiling, HudCorner, HudMode, HudScale,
    HudSettings, INDIGO, MENU_H, MENU_W, MUTED, SEP, Snapshot, VIOLET, WELL_BG, WELL_BORDER, WHITE, client_rect_px,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MenuTab {
    Overlay,
    Lighting,
    Pacing,
    Lab,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActiveSlider {
    AmbientLevel,
    AmbientFloor,
    AmbientAir,
    AmbientReflect,
    AmbientShade,
    TexStageMs,
    TexStageMb,
    RamFloorGb,
    MouseSens,
}

#[derive(Clone, Copy)]
struct Rect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

impl Rect {
    const fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    fn contains(self, px: i32, py: i32) -> bool {
        px >= self.x as i32
            && py >= self.y as i32
            && px < (self.x + self.w) as i32
            && py < (self.y + self.h) as i32
    }
}

pub(super) struct MenuState {
    pub(super) open: bool,
    pub(super) tab: MenuTab,
    focus_row: usize,
    active_slider: Option<ActiveSlider>,
    hover_pt: Option<(i32, i32)>,
    was_lbutton: bool,
    was_tilde: bool,
    was_esc: bool,
    was_tab: bool,
    was_up: bool,
    was_down: bool,
    was_left: bool,
    was_right: bool,
    was_enter: bool,
    last_repeat: Instant,
}

impl MenuState {
    pub(super) fn new() -> Self {
        Self {
            open: false,
            tab: MenuTab::Overlay,
            focus_row: 0,
            active_slider: None,
            hover_pt: None,
            was_lbutton: false,
            was_tilde: false,
            was_esc: false,
            was_tab: false,
            was_up: false,
            was_down: false,
            was_left: false,
            was_right: false,
            was_enter: false,
            last_repeat: Instant::now(),
        }
    }

    fn rows_in_tab(&self) -> usize {
        match self.tab {
            MenuTab::Overlay => 6,
            MenuTab::Lighting => 7,
            MenuTab::Pacing => 6,
            MenuTab::Lab => 6,
        }
    }

    /// Polls toggle (`~`), close (`Esc`), mouse interactions, and keyboard navigation.
    /// Returns `true` when the menu canvas should be re-rendered.
    pub(super) fn poll(
        &mut self,
        hwnd: isize,
        win_size: (u32, u32),
        scale: u32,
        hud: &mut HudSettings,
    ) -> bool {
        if !is_foreground() {
            self.was_tilde = false;
            self.was_lbutton = false;
            self.active_slider = None;
            if self.open {
                self.open = false;
                crate::live_controls::set_menu_open(false);
                return true;
            }
            return false;
        }

        let mut dirty = false;
        let tilde = key_down(i32::from(VK_OEM_3));
        if tilde && !self.was_tilde {
            self.open = !self.open;
            self.active_slider = None;
            crate::live_controls::set_menu_open(self.open);
            dirty = true;
        }
        self.was_tilde = tilde;

        if !self.open {
            self.was_esc = false;
            self.was_lbutton = false;
            self.active_slider = None;
            return dirty;
        }

        let esc = key_down(i32::from(VK_ESCAPE));
        if esc && !self.was_esc {
            self.open = false;
            self.active_slider = None;
            crate::live_controls::set_menu_open(false);
            self.was_esc = esc;
            return true;
        }
        self.was_esc = esc;

        // Keyboard navigation (Tab / Up / Down / Left / Right / Enter).
        let tab_k = key_down(i32::from(VK_TAB));
        if tab_k && !self.was_tab {
            self.tab = match self.tab {
                MenuTab::Overlay => MenuTab::Lighting,
                MenuTab::Lighting => MenuTab::Pacing,
                MenuTab::Pacing => MenuTab::Lab,
                MenuTab::Lab => MenuTab::Overlay,
            };
            self.focus_row = 0;
            dirty = true;
        }
        self.was_tab = tab_k;

        let up_k = key_down(i32::from(VK_UP));
        if up_k && !self.was_up {
            let n = self.rows_in_tab();
            self.focus_row = if self.focus_row == 0 { n - 1 } else { self.focus_row - 1 };
            dirty = true;
        }
        self.was_up = up_k;

        let down_k = key_down(i32::from(VK_DOWN));
        if down_k && !self.was_down {
            self.focus_row = (self.focus_row + 1) % self.rows_in_tab();
            dirty = true;
        }
        self.was_down = down_k;

        let left_k = key_down(i32::from(VK_LEFT));
        let right_k = key_down(i32::from(VK_RIGHT));
        let enter_k = key_down(i32::from(VK_RETURN));
        let repeat_ok = self.last_repeat.elapsed().as_millis() >= 90;
        let step_dir: i32 = if (left_k && !self.was_left) || (left_k && repeat_ok) {
            self.last_repeat = Instant::now();
            -1
        } else if (right_k && !self.was_right) || (right_k && repeat_ok) {
            self.last_repeat = Instant::now();
            1
        } else if enter_k && !self.was_enter {
            1
        } else {
            0
        };
        self.was_left = left_k;
        self.was_right = right_k;
        self.was_enter = enter_k;

        if step_dir != 0 {
            self.apply_key_step(step_dir, hud);
            dirty = true;
        }

        // Mouse coordinate mapping into centered MENU_W x MENU_H logical coordinates.
        let lbtn = key_down(i32::from(VK_LBUTTON));
        let clicked = lbtn && !self.was_lbutton;
        if let Some((lx, ly)) = menu_cursor_logical(hwnd, win_size, scale) {
            if self.hover_pt != Some((lx, ly)) {
                self.hover_pt = Some((lx, ly));
                dirty = true;
            }
            if !lbtn {
                if self.active_slider.is_some() {
                    self.active_slider = None;
                    dirty = true;
                }
            } else {
                if clicked {
                    if self.handle_click(lx, ly, hud) {
                        dirty = true;
                    }
                } else if let Some(slider) = self.active_slider
                    && apply_slider_drag(slider, lx)
                {
                    dirty = true;
                }
            }
        } else if !lbtn && self.active_slider.is_some() {
            self.active_slider = None;
            dirty = true;
        }
        self.was_lbutton = lbtn;

        dirty
    }

    fn apply_key_step(&mut self, dir: i32, hud: &mut HudSettings) {
        match (self.tab, self.focus_row) {
            (MenuTab::Overlay, 0) => {
                let order = [HudMode::Off, HudMode::Basic, HudMode::Advanced];
                let idx = order.iter().position(|&m| m == hud.mode).unwrap_or(1) as i32;
                hud.mode = order[((idx + dir).rem_euclid(3)) as usize];
            }
            (MenuTab::Overlay, 1) => {
                let order = [HudCorner::TopLeft, HudCorner::TopRight, HudCorner::BottomLeft, HudCorner::BottomRight];
                let idx = order.iter().position(|&c| c == hud.corner).unwrap_or(0) as i32;
                hud.corner = order[((idx + dir).rem_euclid(4)) as usize];
            }
            (MenuTab::Overlay, 2) => {
                let order = [GraphCeiling::Ms50, GraphCeiling::Ms100, GraphCeiling::Ms200];
                let idx = order.iter().position(|&c| c == hud.ceiling).unwrap_or(1) as i32;
                hud.ceiling = order[((idx + dir).rem_euclid(3)) as usize];
            }
            (MenuTab::Overlay, 3) => hud.target_fps = if hud.target_fps == 30 { 60 } else { 30 },
            (MenuTab::Overlay, 4) => {
                let order = [HudScale::Auto, HudScale::X1, HudScale::X2];
                let idx = order.iter().position(|&s| s == hud.scale).unwrap_or(0) as i32;
                hud.scale = order[((idx + dir).rem_euclid(3)) as usize];
            }
            (MenuTab::Overlay, 5) => *hud = HudSettings::default(),

            (MenuTab::Lighting, 0) => {
                let cur = crate::ambient::live_state().enabled;
                crate::ambient::set_live_enabled(!cur);
            }
            (MenuTab::Lighting, 1) => {
                let cur = crate::ambient::live_state().level;
                crate::ambient::set_live_level((cur + dir as f32 * 0.05).clamp(0.0, 2.0));
            }
            (MenuTab::Lighting, 2) => {
                let cur = crate::ambient::live_state().floor;
                crate::ambient::set_live_floor((cur + dir as f32 * 0.02).clamp(0.0, 0.5));
            }
            (MenuTab::Lighting, 3) => {
                let cur = crate::ambient::live_state().air;
                crate::ambient::set_live_air((cur + dir as f32 * 0.05).clamp(0.0, 1.5));
            }
            (MenuTab::Lighting, 4) => {
                let cur = crate::ambient::live_state().reflect;
                crate::ambient::set_live_reflect((cur + dir as f32 * 0.1).clamp(0.0, 3.0));
            }
            (MenuTab::Lighting, 5) => {
                let cur = crate::ambient::live_state().shade;
                crate::ambient::set_live_shade((cur + dir as f32 * 0.05).clamp(0.0, 2.5));
            }
            (MenuTab::Lighting, 6) => crate::ambient::reset_live(),

            (MenuTab::Pacing, 0) => {
                let idx = pacing_preset_idx() as i32;
                apply_pacing_preset(((idx + dir).rem_euclid(3)) as usize);
            }
            (MenuTab::Pacing, 1) => {
                let cur = crate::live_controls::tex_stage_ms() as i32;
                crate::live_controls::set_tex_stage_ms((cur + dir * 2).clamp(4, 250) as u64);
            }
            (MenuTab::Pacing, 2) => {
                let cur = crate::live_controls::tex_stage_mb() as i32;
                crate::live_controls::set_tex_stage_mb((cur + dir * 8).clamp(0, 256) as u64);
            }
            (MenuTab::Pacing, 3) => {
                crate::live_controls::set_tex_catchup(!crate::live_controls::tex_catchup());
            }
            (MenuTab::Pacing, 4) => {
                let cur = crate::live_controls::pressure_free_mb() as i32;
                crate::live_controls::set_pressure_free_mb((cur + dir * 256).clamp(1536, 4096) as u64);
            }
            (MenuTab::Pacing, 5) => crate::live_controls::reset_pacing_defaults(),

            (MenuTab::Lab, 0) => crate::live_controls::set_tearing_enabled(!crate::live_controls::tearing_enabled()),
            (MenuTab::Lab, 1) => crate::live_controls::set_kbm_enabled(!crate::live_controls::kbm_enabled()),
            (MenuTab::Lab, 2) => {
                let cur = crate::live_controls::mouse_sens();
                crate::live_controls::set_mouse_sens((cur + dir as f32 * 0.1).clamp(0.2, 3.0));
            }
            (MenuTab::Lab, 3) => crate::live_controls::set_mouse_invert_y(!crate::live_controls::mouse_invert_y()),
            (MenuTab::Lab, 4) => crate::draw::set_live_indirect_clamp(!crate::draw::live_indirect_clamp()),
            (MenuTab::Lab, 5) => {
                *hud = HudSettings::default();
                crate::ambient::reset_live();
                crate::live_controls::reset_defaults();
            }
            _ => {}
        }
    }

    fn handle_click(&mut self, lx: i32, ly: i32, hud: &mut HudSettings) -> bool {
        // Close button [X]
        if Rect::new(480, 14, 24, 24).contains(lx, ly) {
            self.open = false;
            self.active_slider = None;
            crate::live_controls::set_menu_open(false);
            return true;
        }
        // Tabs
        for (i, tab) in [MenuTab::Overlay, MenuTab::Lighting, MenuTab::Pacing, MenuTab::Lab].iter().enumerate() {
            let r = tab_rect(i);
            if r.contains(lx, ly) {
                self.tab = *tab;
                self.focus_row = 0;
                self.active_slider = None;
                return true;
            }
        }

        match self.tab {
            MenuTab::Overlay => {
                if let Some(idx) = hit_seg(lx, ly, row_y(0), 3) {
                    hud.mode = [HudMode::Off, HudMode::Basic, HudMode::Advanced][idx];
                    self.focus_row = 0;
                    return true;
                }
                if let Some(idx) = hit_seg(lx, ly, row_y(1), 4) {
                    hud.corner = [HudCorner::TopLeft, HudCorner::TopRight, HudCorner::BottomLeft, HudCorner::BottomRight][idx];
                    self.focus_row = 1;
                    return true;
                }
                if let Some(idx) = hit_seg(lx, ly, row_y(2), 3) {
                    hud.ceiling = [GraphCeiling::Ms50, GraphCeiling::Ms100, GraphCeiling::Ms200][idx];
                    self.focus_row = 2;
                    return true;
                }
                if let Some(idx) = hit_seg(lx, ly, row_y(3), 2) {
                    hud.target_fps = [30, 60][idx];
                    self.focus_row = 3;
                    return true;
                }
                if let Some(idx) = hit_seg(lx, ly, row_y(4), 3) {
                    hud.scale = [HudScale::Auto, HudScale::X1, HudScale::X2][idx];
                    self.focus_row = 4;
                    return true;
                }
                if reset_btn_rect(row_y(5)).contains(lx, ly) {
                    *hud = HudSettings::default();
                    self.focus_row = 5;
                    return true;
                }
            }
            MenuTab::Lighting => {
                if toggle_rect(row_y(0)).contains(lx, ly) {
                    crate::ambient::set_live_enabled(!crate::ambient::live_state().enabled);
                    self.focus_row = 0;
                    return true;
                }
                let sliders = [
                    (1, ActiveSlider::AmbientLevel, 0.0f32, 2.0f32, 0.05f32, crate::ambient::live_state().level),
                    (2, ActiveSlider::AmbientFloor, 0.0, 0.5, 0.02, crate::ambient::live_state().floor),
                    (3, ActiveSlider::AmbientAir, 0.0, 1.5, 0.05, crate::ambient::live_state().air),
                    (4, ActiveSlider::AmbientReflect, 0.0, 3.0, 0.1, crate::ambient::live_state().reflect),
                    (5, ActiveSlider::AmbientShade, 0.0, 2.5, 0.05, crate::ambient::live_state().shade),
                ];
                for &(row, id, min, max, step, cur) in &sliders {
                    let y = row_y(row);
                    if slider_minus_rect(y).contains(lx, ly) {
                        set_slider_val(id, (cur - step).clamp(min, max));
                        self.focus_row = row;
                        return true;
                    }
                    if slider_plus_rect(y).contains(lx, ly) {
                        set_slider_val(id, (cur + step).clamp(min, max));
                        self.focus_row = row;
                        return true;
                    }
                    if slider_track_hit_rect(y).contains(lx, ly) {
                        self.active_slider = Some(id);
                        self.focus_row = row;
                        apply_slider_drag(id, lx);
                        return true;
                    }
                }
                if reset_btn_rect(row_y(6)).contains(lx, ly) {
                    crate::ambient::reset_live();
                    self.focus_row = 6;
                    return true;
                }
            }
            MenuTab::Pacing => {
                if let Some(idx) = hit_seg(lx, ly, row_y(0), 3) {
                    apply_pacing_preset(idx);
                    self.focus_row = 0;
                    return true;
                }
                let sliders = [
                    (1, ActiveSlider::TexStageMs, 4.0f32, 250.0f32, 2.0f32, crate::live_controls::tex_stage_ms() as f32),
                    (2, ActiveSlider::TexStageMb, 0.0, 256.0, 8.0, crate::live_controls::tex_stage_mb() as f32),
                    (
                        4,
                        ActiveSlider::RamFloorGb,
                        1.5,
                        4.0,
                        0.25,
                        crate::live_controls::pressure_free_mb() as f32 / 1024.0,
                    ),
                ];
                for &(row, id, min, max, step, cur) in &sliders {
                    let y = row_y(row);
                    if slider_minus_rect(y).contains(lx, ly) {
                        set_slider_val(id, (cur - step).clamp(min, max));
                        self.focus_row = row;
                        return true;
                    }
                    if slider_plus_rect(y).contains(lx, ly) {
                        set_slider_val(id, (cur + step).clamp(min, max));
                        self.focus_row = row;
                        return true;
                    }
                    if slider_track_hit_rect(y).contains(lx, ly) {
                        self.active_slider = Some(id);
                        self.focus_row = row;
                        apply_slider_drag(id, lx);
                        return true;
                    }
                }
                if toggle_rect(row_y(3)).contains(lx, ly) {
                    crate::live_controls::set_tex_catchup(!crate::live_controls::tex_catchup());
                    self.focus_row = 3;
                    return true;
                }
                if reset_btn_rect(row_y(5)).contains(lx, ly) {
                    crate::live_controls::reset_pacing_defaults();
                    self.focus_row = 5;
                    return true;
                }
            }
            MenuTab::Lab => {
                if toggle_rect(row_y(0)).contains(lx, ly) {
                    crate::live_controls::set_tearing_enabled(!crate::live_controls::tearing_enabled());
                    self.focus_row = 0;
                    return true;
                }
                if toggle_rect(row_y(1)).contains(lx, ly) {
                    crate::live_controls::set_kbm_enabled(!crate::live_controls::kbm_enabled());
                    self.focus_row = 1;
                    return true;
                }
                let y = row_y(2);
                let sens = crate::live_controls::mouse_sens();
                if slider_minus_rect(y).contains(lx, ly) {
                    crate::live_controls::set_mouse_sens((sens - 0.1).clamp(0.2, 3.0));
                    self.focus_row = 2;
                    return true;
                }
                if slider_plus_rect(y).contains(lx, ly) {
                    crate::live_controls::set_mouse_sens((sens + 0.1).clamp(0.2, 3.0));
                    self.focus_row = 2;
                    return true;
                }
                if slider_track_hit_rect(y).contains(lx, ly) {
                    self.active_slider = Some(ActiveSlider::MouseSens);
                    self.focus_row = 2;
                    apply_slider_drag(ActiveSlider::MouseSens, lx);
                    return true;
                }
                if toggle_rect(row_y(3)).contains(lx, ly) {
                    crate::live_controls::set_mouse_invert_y(!crate::live_controls::mouse_invert_y());
                    self.focus_row = 3;
                    return true;
                }
                if wide_toggle_rect(row_y(4)).contains(lx, ly) {
                    crate::draw::set_live_indirect_clamp(!crate::draw::live_indirect_clamp());
                    self.focus_row = 4;
                    return true;
                }
                if reset_btn_rect(row_y(5)).contains(lx, ly) {
                    *hud = HudSettings::default();
                    crate::ambient::reset_live();
                    crate::live_controls::reset_defaults();
                    self.focus_row = 5;
                    return true;
                }
            }
        }
        false
    }
}

fn pacing_preset_idx() -> usize {
    let ms = crate::live_controls::tex_stage_ms();
    let catchup = crate::live_controls::tex_catchup();
    if !catchup && ms <= 10 {
        0
    } else if !catchup && ms <= 24 {
        1
    } else {
        2
    }
}

fn apply_pacing_preset(idx: usize) {
    match idx {
        0 => {
            crate::live_controls::set_tex_stage_ms(8);
            crate::live_controls::set_tex_stage_mb(32);
            crate::live_controls::set_tex_catchup(false);
            crate::live_controls::set_pressure_free_mb(2816);
        }
        1 => {
            crate::live_controls::set_tex_stage_ms(14);
            crate::live_controls::set_tex_stage_mb(48);
            crate::live_controls::set_tex_catchup(false);
            crate::live_controls::set_pressure_free_mb(2560);
        }
        _ => {
            crate::live_controls::set_tex_stage_ms(250);
            crate::live_controls::set_tex_stage_mb(0);
            crate::live_controls::set_tex_catchup(true);
            crate::live_controls::set_pressure_free_mb(1536);
        }
    }
}

fn slider_spec(id: ActiveSlider) -> (f32, f32) {
    match id {
        ActiveSlider::AmbientLevel => (0.0, 2.0),
        ActiveSlider::AmbientFloor => (0.0, 0.5),
        ActiveSlider::AmbientAir => (0.0, 1.5),
        ActiveSlider::AmbientReflect => (0.0, 3.0),
        ActiveSlider::AmbientShade => (0.0, 2.5),
        ActiveSlider::TexStageMs => (4.0, 250.0),
        ActiveSlider::TexStageMb => (0.0, 256.0),
        ActiveSlider::RamFloorGb => (1.5, 4.0),
        ActiveSlider::MouseSens => (0.2, 3.0),
    }
}

fn set_slider_val(id: ActiveSlider, val: f32) {
    match id {
        ActiveSlider::AmbientLevel => crate::ambient::set_live_level(val),
        ActiveSlider::AmbientFloor => crate::ambient::set_live_floor(val),
        ActiveSlider::AmbientAir => crate::ambient::set_live_air(val),
        ActiveSlider::AmbientReflect => crate::ambient::set_live_reflect(val),
        ActiveSlider::AmbientShade => crate::ambient::set_live_shade(val),
        ActiveSlider::TexStageMs => crate::live_controls::set_tex_stage_ms(val.round().clamp(4.0, 250.0) as u64),
        ActiveSlider::TexStageMb => {
            let mb = val.round().clamp(0.0, 256.0) as u64;
            crate::live_controls::set_tex_stage_mb(if mb < 4 { 0 } else { mb.clamp(8, 256) });
        }
        ActiveSlider::RamFloorGb => {
            let mib = (val * 1024.0).round().clamp(1536.0, 4096.0) as u64;
            crate::live_controls::set_pressure_free_mb(mib);
        }
        ActiveSlider::MouseSens => crate::live_controls::set_mouse_sens(val),
    }
}

fn apply_slider_drag(id: ActiveSlider, lx: i32) -> bool {
    let (min, max) = slider_spec(id);
    let track = slider_track_rect(0);
    let t = ((lx - track.x as i32) as f32 / track.w as f32).clamp(0.0, 1.0);
    let raw = min + (max - min) * t;
    let stepped = match id {
        ActiveSlider::TexStageMs => (raw / 2.0).round() * 2.0,
        ActiveSlider::TexStageMb => (raw / 8.0).round() * 8.0,
        ActiveSlider::RamFloorGb => (raw * 4.0).round() / 4.0,
        _ => (raw * 100.0).round() / 100.0,
    };
    set_slider_val(id, stepped.clamp(min, max));
    true
}

const fn tab_rect(i: usize) -> Rect {
    Rect::new(18 + i as u32 * 122, 56, 116, 26)
}

const fn row_y(row: usize) -> u32 {
    102 + row as u32 * 48
}

const fn seg_group_rect(y: u32) -> Rect {
    Rect::new(236, y + 6, 256, 26)
}

fn hit_seg(lx: i32, ly: i32, y: u32, count: usize) -> Option<usize> {
    let g = seg_group_rect(y);
    if !g.contains(lx, ly) || count == 0 {
        return None;
    }
    let idx = ((lx - g.x as i32) as usize * count) / g.w as usize;
    Some(idx.min(count - 1))
}

const fn toggle_rect(y: u32) -> Rect {
    Rect::new(408, y + 6, 84, 26)
}

const fn wide_toggle_rect(y: u32) -> Rect {
    Rect::new(360, y + 6, 132, 26)
}

const fn slider_minus_rect(y: u32) -> Rect {
    Rect::new(236, y + 8, 22, 22)
}

const fn slider_track_rect(y: u32) -> Rect {
    Rect::new(266, y + 16, 142, 6)
}

const fn slider_track_hit_rect(y: u32) -> Rect {
    Rect::new(262, y + 6, 150, 26)
}

const fn slider_plus_rect(y: u32) -> Rect {
    Rect::new(416, y + 8, 22, 22)
}

const fn reset_btn_rect(y: u32) -> Rect {
    Rect::new(312, y + 6, 180, 28)
}

pub(super) fn menu_origin(win_size: (u32, u32), scale: u32) -> (u32, u32) {
    let mw = MENU_W * scale;
    let mh = MENU_H * scale;
    (
        win_size.0.saturating_sub(mw) / 2,
        win_size.1.saturating_sub(mh) / 2,
    )
}

fn menu_cursor_logical(hwnd: isize, win_size: (u32, u32), scale: u32) -> Option<(i32, i32)> {
    let (cw, ch) = client_rect_px(hwnd)?;
    if cw == 0 || ch == 0 || win_size.0 == 0 || win_size.1 == 0 {
        return None;
    }
    let mut pt = WIN_POINT::default();
    // SAFETY: valid initialized POINT.
    if unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt) }.is_err() {
        return None;
    }
    // SAFETY: maps screen cursor coordinates into window client coordinates.
    if !unsafe { ScreenToClient(HWND(hwnd as *mut _), &mut pt) }.as_bool() {
        return None;
    }
    let bx = (pt.x as f32 * win_size.0 as f32 / cw as f32).round() as i32;
    let by = (pt.y as f32 * win_size.1 as f32 / ch as f32).round() as i32;
    let (ox, oy) = menu_origin(win_size, scale);
    let lx = (bx - ox as i32) / scale as i32;
    let ly = (by - oy as i32) / scale as i32;
    Some((lx, ly))
}

fn key_down(vk: i32) -> bool {
    // SAFETY: plain async key query.
    unsafe { GetAsyncKeyState(vk) < 0 }
}

fn is_foreground() -> bool {
    let mut process = 0;
    // SAFETY: plain queries; room for process id.
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), &mut process) };
    process == std::process::id()
}

pub(super) fn paint_menu(menu: &MenuState, hud: &HudSettings, snap: Option<&Snapshot>) -> Canvas {
    let mut c = Canvas::new(MENU_W, MENU_H, WELL_BG);
    c.rounded_rect_grad_v(0, 0, MENU_W, MENU_H, 12.0, CARD_TOP, CARD_BOT);
    c.fill_grad_h(12, 1, MENU_W - 24, 2, INDIGO, AMBER);
    c.rounded_rect_stroke(0, 0, MENU_W, MENU_H, 12.0, BORDER, 1.0);

    // Header
    c.draw_vesper_emblem(18, 12, 28);
    c.text(56, 12, 2, WHITE, "VESPER CONTROL");
    c.text(56, 32, 1, MUTED, "Live Runtime & Visual Tuning");

    if let Some(s) = snap {
        let badge = format!("{:.1} FPS | {:.1}ms", s.fps, s.frame_ms);
        let bw = text_width(&badge, 1) + 18;
        let bx = 468u32.saturating_sub(bw);
        c.rounded_rect_fill(bx, 15, bw, 22, 6.0, WELL_BG, 1.0);
        c.rounded_rect_stroke(bx, 15, bw, 22, 6.0, WELL_BORDER, 1.0);
        c.text(bx + 9, 22, 1, super::fps_color(s.fps), &badge);
    }

    // Close [X]
    let close_r = Rect::new(480, 14, 24, 24);
    let close_hot = menu.hover_pt.is_some_and(|(hx, hy)| close_r.contains(hx, hy));
    c.rounded_rect_fill(close_r.x, close_r.y, close_r.w, close_r.h, 6.0, if close_hot { [58, 28, 42] } else { [22, 28, 48] }, 1.0);
    c.rounded_rect_stroke(close_r.x, close_r.y, close_r.w, close_r.h, 6.0, if close_hot { CORAL } else { WELL_BORDER }, 1.0);
    c.text(close_r.x + 9, close_r.y + 8, 1, if close_hot { CORAL } else { WHITE }, "X");

    // Tab strip
    let tabs = [
        (MenuTab::Overlay, "Overlay"),
        (MenuTab::Lighting, "Lighting"),
        (MenuTab::Pacing, "Pacing"),
        (MenuTab::Lab, "Controls"),
    ];
    for (i, &(tab, lbl)) in tabs.iter().enumerate() {
        let r = tab_rect(i);
        let active = menu.tab == tab;
        let hovered = menu.hover_pt.is_some_and(|(hx, hy)| r.contains(hx, hy));
        let fill = if active { [34, 38, 72] } else if hovered { [24, 29, 54] } else { [15, 19, 34] };
        let stroke = if active { INDIGO } else { WELL_BORDER };
        c.rounded_rect_fill(r.x, r.y, r.w, r.h, 6.0, fill, 1.0);
        c.rounded_rect_stroke(r.x, r.y, r.w, r.h, 6.0, stroke, 1.0);
        if active {
            c.fill_grad_h(r.x + 12, r.y + r.h - 2, r.w - 24, 2, INDIGO, VIOLET);
        }
        let tw = text_width(lbl, 1);
        c.text(r.x + (r.w - tw) / 2, r.y + 9, 1, if active { WHITE } else { MUTED }, lbl);
    }

    // Content body well
    c.rounded_rect_fill(14, 92, MENU_W - 28, 360, 8.0, WELL_BG, 1.0);
    c.rounded_rect_stroke(14, 92, MENU_W - 28, 360, 8.0, WELL_BORDER, 1.0);

    match menu.tab {
        MenuTab::Overlay => {
            draw_row_bg(&mut c, 0, menu.focus_row == 0, "HUD Mode", "F3 cycles modes anytime");
            let mode_idx = match hud.mode {
                HudMode::Off => 0,
                HudMode::Basic => 1,
                HudMode::Advanced => 2,
            };
            draw_segmented(&mut c, row_y(0), &["Off", "Basic", "Advanced"], mode_idx, menu.hover_pt);

            draw_row_bg(&mut c, 1, menu.focus_row == 1, "HUD Corner", "Anchor corner with 16px margin");
            let corner_idx = match hud.corner {
                HudCorner::TopLeft => 0,
                HudCorner::TopRight => 1,
                HudCorner::BottomLeft => 2,
                HudCorner::BottomRight => 3,
            };
            draw_segmented(&mut c, row_y(1), &["Top-L", "Top-R", "Bot-L", "Bot-R"], corner_idx, menu.hover_pt);

            draw_row_bg(&mut c, 2, menu.focus_row == 2, "Graph Ceiling", "Vertical frame-time ms scale");
            let ceil_idx = match hud.ceiling {
                GraphCeiling::Ms50 => 0,
                GraphCeiling::Ms100 => 1,
                GraphCeiling::Ms200 => 2,
            };
            draw_segmented(&mut c, row_y(2), &["50ms", "100ms", "200ms"], ceil_idx, menu.hover_pt);

            draw_row_bg(&mut c, 3, menu.focus_row == 3, "Target Reference", "Dashed target pace guide");
            let tgt_idx = if hud.target_fps >= 60 { 1 } else { 0 };
            draw_segmented(&mut c, row_y(3), &["30 FPS", "60 FPS"], tgt_idx, menu.hover_pt);

            draw_row_bg(&mut c, 4, menu.focus_row == 4, "UI Scale", "2x auto-activates at 4K");
            let scale_idx = match hud.scale {
                HudScale::Auto => 0,
                HudScale::X1 => 1,
                HudScale::X2 => 2,
            };
            draw_segmented(&mut c, row_y(4), &["Auto", "1x", "2x"], scale_idx, menu.hover_pt);

            draw_row_bg(&mut c, 5, menu.focus_row == 5, "Defaults", "Restore clean HUD layout");
            draw_action_button(&mut c, row_y(5), "Reset Overlay Defaults", menu.hover_pt);
        }
        MenuTab::Lighting => {
            let st = crate::ambient::live_state();
            draw_row_bg(&mut c, 0, menu.focus_row == 0, "Ambient Stand-In", "Live D3D12 indirect estimate");
            draw_toggle(&mut c, row_y(0), st.enabled, menu.hover_pt);

            draw_row_bg(&mut c, 1, menu.focus_row == 1, "Ambient Strength", "Scene diffuse lift (default 0.80)");
            draw_slider(&mut c, row_y(1), st.level, 0.0, 2.0, menu.hover_pt, menu.active_slider == Some(ActiveSlider::AmbientLevel));

            draw_row_bg(&mut c, 2, menu.focus_row == 2, "Interior Floor", "Minimum dark-room ambient lift");
            draw_slider(&mut c, row_y(2), st.floor, 0.0, 0.5, menu.hover_pt, menu.active_slider == Some(ActiveSlider::AmbientFloor));

            draw_row_bg(&mut c, 3, menu.focus_row == 3, "Air Scatter", "Atmospheric volume veil multiple");
            draw_slider(&mut c, row_y(3), st.air, 0.0, 1.5, menu.hover_pt, menu.active_slider == Some(ActiveSlider::AmbientAir));

            draw_row_bg(&mut c, 4, menu.focus_row == 4, "Reflection Clamp", "Caps specular bounce energy");
            draw_slider(&mut c, row_y(4), st.reflect, 0.0, 3.0, menu.hover_pt, menu.active_slider == Some(ActiveSlider::AmbientReflect));

            draw_row_bg(&mut c, 5, menu.focus_row == 5, "Crease Shade", "Ambient occlusion contact shadow");
            draw_slider(&mut c, row_y(5), st.shade, 0.0, 2.5, menu.hover_pt, menu.active_slider == Some(ActiveSlider::AmbientShade));

            draw_row_bg(&mut c, 6, menu.focus_row == 6, "Lighting Defaults", "Restore calibrated startup values");
            draw_action_button(&mut c, row_y(6), "Reset Lighting Defaults", menu.hover_pt);
        }
        MenuTab::Pacing => {
            draw_row_bg(&mut c, 0, menu.focus_row == 0, "Pacing Preset", "Instant anti-stutter tuning");
            draw_segmented(&mut c, row_y(0), &["Ultra 8ms", "Smooth 14ms", "Stock"], pacing_preset_idx(), menu.hover_pt);

            let stage_ms = crate::live_controls::tex_stage_ms();
            let ms_lbl = format!("{stage_ms}ms");
            draw_row_bg(&mut c, 1, menu.focus_row == 1, "Tex Stage Cap", "Max CPU ms/frame for textures");
            draw_slider_fmt(
                &mut c,
                row_y(1),
                (stage_ms as f32).clamp(4.0, 250.0),
                4.0,
                250.0,
                &ms_lbl,
                menu.hover_pt,
                menu.active_slider == Some(ActiveSlider::TexStageMs),
            );

            let stage_mb = crate::live_controls::tex_stage_mb();
            let mb_lbl = if stage_mb == 0 { String::from("Auto") } else { format!("{stage_mb}MB") };
            draw_row_bg(&mut c, 2, menu.focus_row == 2, "Tex Budget/Frame", "MB/frame ceiling (0 = Auto)");
            draw_slider_fmt(
                &mut c,
                row_y(2),
                (stage_mb as f32).clamp(0.0, 256.0),
                0.0,
                256.0,
                &mb_lbl,
                menu.hover_pt,
                menu.active_slider == Some(ActiveSlider::TexStageMb),
            );

            let catchup = crate::live_controls::tex_catchup();
            draw_row_bg(&mut c, 3, menu.focus_row == 3, "Backlog 3x Catchup", "Off prevents 750ms upload stalls");
            draw_toggle(&mut c, row_y(3), catchup, menu.hover_pt);

            let floor_gb = crate::live_controls::pressure_free_mb() as f32 / 1024.0;
            let gb_lbl = format!("{floor_gb:.2}GB");
            draw_row_bg(&mut c, 4, menu.focus_row == 4, "RAM Trim Floor", "Free RAM threshold before swap");
            draw_slider_fmt(
                &mut c,
                row_y(4),
                floor_gb.clamp(1.5, 4.0),
                1.5,
                4.0,
                &gb_lbl,
                menu.hover_pt,
                menu.active_slider == Some(ActiveSlider::RamFloorGb),
            );

            draw_row_bg(&mut c, 5, menu.focus_row == 5, "Pacing Defaults", "Restore startup pacing switches");
            draw_action_button(&mut c, row_y(5), "Reset Pacing Defaults", menu.hover_pt);
        }
        MenuTab::Lab => {
            let tearing = crate::live_controls::tearing_enabled();
            draw_row_bg(&mut c, 0, menu.focus_row == 0, "Allow Tearing", "Borderless low-latency present");
            draw_toggle(&mut c, row_y(0), tearing, menu.hover_pt);

            let kbm = crate::live_controls::kbm_enabled();
            draw_row_bg(&mut c, 1, menu.focus_row == 1, "PC KBM Input", "Mouse look + LMB/RMB triggers");
            draw_toggle(&mut c, row_y(1), kbm, menu.hover_pt);

            let sens = crate::live_controls::mouse_sens();
            draw_row_bg(&mut c, 2, menu.focus_row == 2, "Mouse Sensitivity", "Right-stick look multiplier");
            draw_slider(&mut c, row_y(2), sens, 0.2, 3.0, menu.hover_pt, menu.active_slider == Some(ActiveSlider::MouseSens));

            let inv = crate::live_controls::mouse_invert_y();
            draw_row_bg(&mut c, 3, menu.focus_row == 3, "Invert Mouse Y", "Flip vertical camera pitch");
            draw_toggle(&mut c, row_y(3), inv, menu.hover_pt);

            let clamp = crate::draw::live_indirect_clamp();
            draw_row_bg(&mut c, 4, menu.focus_row == 4, "Indirect Arg Clamp", "Unsafe off can corrupt skin");
            draw_safety_toggle(&mut c, row_y(4), clamp, menu.hover_pt);

            draw_row_bg(&mut c, 5, menu.focus_row == 5, "All Live Settings", "Restore all tabs to startup state");
            draw_action_button(&mut c, row_y(5), "Reset All Defaults", menu.hover_pt);
        }
    }

    // Footer hints
    let footer = "~ Toggle Menu  |  Tab Switch Tab  |  F3 Cycle HUD  |  F2 Mouse Look";
    c.text_sep(28, 466, 1, MUTED, SEP, footer);

    c
}

fn draw_row_bg(c: &mut Canvas, row: usize, focused: bool, title: &str, subtitle: &str) {
    let y = row_y(row);
    let fill = if focused { [20, 25, 45] } else if row.is_multiple_of(2) { [12, 16, 29] } else { [10, 13, 24] };
    c.rounded_rect_fill(20, y, MENU_W - 40, 40, 6.0, fill, 1.0);
    if focused {
        c.rounded_rect_stroke(20, y, MENU_W - 40, 40, 6.0, INDIGO, 0.7);
    }
    c.text(30, y + 7, 1, WHITE, title);
    c.text(30, y + 23, 1, MUTED, subtitle);
}

fn draw_segmented(c: &mut Canvas, y: u32, labels: &[&str], active: usize, hover: Option<(i32, i32)>) {
    let g = seg_group_rect(y);
    c.rounded_rect_fill(g.x, g.y, g.w, g.h, 6.0, [16, 21, 38], 1.0);
    c.rounded_rect_stroke(g.x, g.y, g.w, g.h, 6.0, WELL_BORDER, 1.0);
    let n = labels.len() as u32;
    let cell_w = g.w / n;
    for (i, &lbl) in labels.iter().enumerate() {
        let cx = g.x + i as u32 * cell_w + 2;
        let cw = if i + 1 == labels.len() { g.x + g.w - 2 - cx } else { cell_w - 4 };
        let r = Rect::new(cx, g.y + 2, cw, g.h - 4);
        let is_active = i == active;
        let is_hover = hover.is_some_and(|(hx, hy)| r.contains(hx, hy));
        if is_active {
            c.rounded_rect_grad_v(r.x, r.y, r.w, r.h, 4.0, [79, 86, 200], [116, 62, 185]);
        } else if is_hover {
            c.rounded_rect_fill(r.x, r.y, r.w, r.h, 4.0, [28, 35, 62], 1.0);
        }
        let tw = text_width(lbl, 1);
        c.text(r.x + r.w.saturating_sub(tw) / 2, r.y + 7, 1, if is_active { WHITE } else { MUTED }, lbl);
    }
}

fn draw_toggle(c: &mut Canvas, y: u32, on: bool, hover: Option<(i32, i32)>) {
    let r = toggle_rect(y);
    let hot = hover.is_some_and(|(hx, hy)| r.contains(hx, hy));
    let fill = if on { [18, 62, 54] } else { [26, 31, 48] };
    let border = if on { EMERALD } else if hot { INDIGO } else { WELL_BORDER };
    c.rounded_rect_fill(r.x, r.y, r.w, r.h, 12.0, fill, 1.0);
    c.rounded_rect_stroke(r.x, r.y, r.w, r.h, 12.0, border, 1.0);
    let knob_x = if on { r.x + r.w - 14 } else { r.x + 14 };
    c.circle_aa(knob_x as f32, (r.y + r.h / 2) as f32, 7.5, if on { EMERALD } else { MUTED }, 1.0);
    let lbl = if on { "ON" } else { "OFF" };
    let lx = if on { r.x + 14 } else { r.x + r.w - 34 };
    c.text(lx, r.y + 9, 1, if on { WHITE } else { MUTED }, lbl);
}

fn draw_safety_toggle(c: &mut Canvas, y: u32, safe_on: bool, hover: Option<(i32, i32)>) {
    let r = wide_toggle_rect(y);
    let hot = hover.is_some_and(|(hx, hy)| r.contains(hx, hy));
    let fill = if safe_on { [18, 62, 54] } else { [74, 30, 34] };
    let border = if safe_on { EMERALD } else if hot { GOLD } else { CORAL };
    c.rounded_rect_fill(r.x, r.y, r.w, r.h, 12.0, fill, 1.0);
    c.rounded_rect_stroke(r.x, r.y, r.w, r.h, 12.0, border, 1.0);
    let lbl = if safe_on { "SAFE: ON" } else { "UNSAFE: OFF" };
    let tw = text_width(lbl, 1);
    c.text(r.x + (r.w - tw) / 2, r.y + 9, 1, if safe_on { EMERALD } else { CORAL }, lbl);
}

fn draw_slider(c: &mut Canvas, y: u32, val: f32, min: f32, max: f32, hover: Option<(i32, i32)>, dragging: bool) {
    let vstr = format!("{val:.2}");
    draw_slider_fmt(c, y, val, min, max, &vstr, hover, dragging);
}

#[allow(clippy::too_many_arguments)]
fn draw_slider_fmt(
    c: &mut Canvas,
    y: u32,
    val: f32,
    min: f32,
    max: f32,
    vstr: &str,
    hover: Option<(i32, i32)>,
    dragging: bool,
) {
    let minus = slider_minus_rect(y);
    let plus = slider_plus_rect(y);
    let track = slider_track_rect(y);

    for (r, sym) in [(minus, "-"), (plus, "+")] {
        let hot = hover.is_some_and(|(hx, hy)| r.contains(hx, hy));
        c.rounded_rect_fill(r.x, r.y, r.w, r.h, 5.0, if hot { [36, 44, 78] } else { [20, 26, 46] }, 1.0);
        c.rounded_rect_stroke(r.x, r.y, r.w, r.h, 5.0, if hot { INDIGO } else { WELL_BORDER }, 1.0);
        c.text(r.x + 8, r.y + 7, 1, WHITE, sym);
    }

    c.rounded_rect_fill(track.x, track.y, track.w, track.h, 3.0, [24, 31, 52], 1.0);
    let t = ((val - min) / (max - min).max(1e-4)).clamp(0.0, 1.0);
    let fill_w = ((t * track.w as f32).round() as u32).max(4);
    c.fill_grad_h(track.x, track.y, fill_w.min(track.w), track.h, INDIGO, AMBER);
    let thumb_x = track.x as f32 + t * track.w as f32;
    let thumb_y = track.y as f32 + track.h as f32 * 0.5;
    c.circle_aa(thumb_x, thumb_y, if dragging { 7.0 } else { 5.8 }, WHITE, 1.0);
    c.circle_aa(thumb_x, thumb_y, 3.0, VIOLET, 1.0);

    c.rounded_rect_fill(444, y + 7, 50, 24, 5.0, [16, 21, 38], 1.0);
    c.rounded_rect_stroke(444, y + 7, 50, 24, 5.0, WELL_BORDER, 1.0);
    let vw = text_width(vstr, 1);
    c.text(444 + (50u32.saturating_sub(vw)) / 2, y + 15, 1, CYAN, vstr);
}

fn draw_action_button(c: &mut Canvas, y: u32, label: &str, hover: Option<(i32, i32)>) {
    let r = reset_btn_rect(y);
    let hot = hover.is_some_and(|(hx, hy)| r.contains(hx, hy));
    c.rounded_rect_fill(r.x, r.y, r.w, r.h, 6.0, if hot { [48, 34, 68] } else { [26, 30, 52] }, 1.0);
    c.rounded_rect_stroke(r.x, r.y, r.w, r.h, 6.0, if hot { AMBER } else { VIOLET }, 1.0);
    let tw = text_width(label, 1);
    c.text(r.x + (r.w - tw) / 2, r.y + 10, 1, WHITE, label);
}
