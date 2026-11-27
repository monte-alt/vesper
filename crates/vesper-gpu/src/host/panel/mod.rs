//! Vesper DX12 telemetry HUD (`Basic` & `Advanced` modes; `F3` cycles) and
//! interactive runtime tuning menu (`~` toggles).

mod draw2d;
mod font;
mod menu;

use std::collections::VecDeque;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_F3};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, GetForegroundWindow, GetWindowThreadProcessId};

use self::draw2d::{Canvas, auto_scale, bpp};
use self::font::{draw_hero_numerals, text_width};
use self::menu::{MenuState, menu_origin, paint_menu};
use super::{Host, HostList, transition};

const EVERY: Duration = Duration::from_millis(125);
const HISTORY: usize = 180;
const UPLOADS: usize = 4;

pub(super) const BASIC_W: u32 = 360;
pub(super) const BASIC_H: u32 = 140;
pub(super) const ADV_W: u32 = 500;
pub(super) const ADV_H: u32 = 416;
pub(super) const MENU_W: u32 = 520;
pub(super) const MENU_H: u32 = 496;

// Vesper Dusk palette (sRGB).
pub(super) const CARD_TOP: [u8; 3] = [11, 15, 27];
pub(super) const CARD_BOT: [u8; 3] = [17, 22, 41];
pub(super) const BORDER: [u8; 3] = [38, 48, 80];
pub(super) const WELL_BG: [u8; 3] = [8, 11, 20];
pub(super) const WELL_BORDER: [u8; 3] = [29, 37, 61];
pub(super) const WHITE: [u8; 3] = [241, 245, 249];
pub(super) const MUTED: [u8; 3] = [148, 163, 184];
pub(super) const SEP: [u8; 3] = [71, 85, 105];
pub(super) const INDIGO: [u8; 3] = [129, 140, 248];
pub(super) const VIOLET: [u8; 3] = [168, 85, 247];
pub(super) const AMBER: [u8; 3] = [251, 146, 60];
pub(super) const EMERALD: [u8; 3] = [52, 211, 153];
pub(super) const GOLD: [u8; 3] = [251, 191, 36];
pub(super) const CORAL: [u8; 3] = [248, 113, 113];
pub(super) const CYAN: [u8; 3] = [56, 189, 248];
pub(super) const BAR_BG: [u8; 3] = [26, 33, 54];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum HudMode {
    Off,
    Basic,
    Advanced,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum HudCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum GraphCeiling {
    Ms50,
    Ms100,
    Ms200,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum HudScale {
    Auto,
    X1,
    X2,
}

pub(super) struct HudSettings {
    pub(super) mode: HudMode,
    pub(super) corner: HudCorner,
    pub(super) ceiling: GraphCeiling,
    pub(super) target_fps: u32,
    pub(super) scale: HudScale,
}

impl Default for HudSettings {
    fn default() -> Self {
        Self {
            mode: if asked() { HudMode::Basic } else { HudMode::Off },
            corner: HudCorner::TopLeft,
            ceiling: GraphCeiling::Ms100,
            target_fps: 30,
            scale: HudScale::Auto,
        }
    }
}

impl HudSettings {
    fn effective_scale(&self, height: u32) -> u32 {
        match self.scale {
            HudScale::Auto => auto_scale(height),
            HudScale::X1 => 1,
            HudScale::X2 => 2,
        }
    }

    fn ceiling_ms(&self) -> f32 {
        match self.ceiling {
            GraphCeiling::Ms50 => 50.0,
            GraphCeiling::Ms100 => 100.0,
            GraphCeiling::Ms200 => 200.0,
        }
    }

    fn dims(&self) -> (u32, u32) {
        match self.mode {
            HudMode::Advanced => (ADV_W, ADV_H),
            HudMode::Basic | HudMode::Off => (BASIC_W, BASIC_H),
        }
    }
}

fn asked() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| crate::switch::on("VESPER_PANEL"))
}

fn f3_pressed() -> bool {
    static DOWN: AtomicBool = AtomicBool::new(false);
    // SAFETY: plain async key query.
    let down = unsafe { GetAsyncKeyState(i32::from(VK_F3)) } < 0;
    if DOWN.swap(down, Ordering::Relaxed) || !down {
        return false;
    }
    let mut process = 0;
    // SAFETY: plain queries; room for process id.
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), &mut process) };
    process == std::process::id()
}

pub(super) fn client_rect_px(hwnd: isize) -> Option<(u32, u32)> {
    let mut r = RECT::default();
    // SAFETY: plain window client query.
    if unsafe { GetClientRect(hwnd as _, &mut r) } == 0 {
        return None;
    }
    Some(((r.right - r.left).max(0) as u32, (r.bottom - r.top).max(0) as u32))
}

fn process_private_bytes() -> u64 {
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut c = PROCESS_MEMORY_COUNTERS {
        cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    // SAFETY: our own process and a sized structure.
    unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    c.PagefileUsage as u64
}

fn cpu_name() -> &'static str {
    static NAME: OnceLock<String> = OnceLock::new();
    NAME.get_or_init(|| {
        #[cfg(target_arch = "x86_64")]
        {
            let mut raw = [0u8; 48];
            for (i, leaf) in (0x8000_0002u32..=0x8000_0004).enumerate() {
                let r = std::arch::x86_64::__cpuid(leaf);
                raw[i * 16..i * 16 + 4].copy_from_slice(&r.eax.to_le_bytes());
                raw[i * 16 + 4..i * 16 + 8].copy_from_slice(&r.ebx.to_le_bytes());
                raw[i * 16 + 8..i * 16 + 12].copy_from_slice(&r.ecx.to_le_bytes());
                raw[i * 16 + 12..i * 16 + 16].copy_from_slice(&r.edx.to_le_bytes());
            }
            let s = String::from_utf8_lossy(&raw);
            let clean = s
                .trim_matches(|c: char| c == '\0' || c.is_whitespace())
                .replace("8-Core Processor", "")
                .replace("16-Core Processor", "")
                .replace("12-Core Processor", "")
                .replace("6-Core Processor", "")
                .replace("Processor", "");
            clean.split_whitespace().collect::<Vec<_>>().join(" ")
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            "CPU".to_owned()
        }
    })
}

fn gpu_info(host: &Host) -> (String, Option<u64>) {
    let Some(adapter) = host.adapter.as_ref() else {
        return ("GPU".to_owned(), None);
    };
    // SAFETY: plain DXGI descriptor query.
    let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
        return ("GPU".to_owned(), None);
    };
    let end = desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len());
    let name = String::from_utf16_lossy(&desc.Description[..end]).trim().to_owned();
    (if name.is_empty() { "GPU".to_owned() } else { name }, Some(desc.DedicatedVideoMemory as u64))
}

fn system_memory() -> (u64, u64, u64, u64) {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    // SAFETY: plain sized query.
    let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    m.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut m) } == 0 {
        return (0, 0, 0, 0);
    }
    (
        m.ullTotalPhys.saturating_sub(m.ullAvailPhys),
        m.ullTotalPhys,
        m.ullTotalPageFile.saturating_sub(m.ullAvailPageFile),
        m.ullTotalPageFile,
    )
}

#[derive(Default)]
struct CpuMeter {
    prev: Vec<(i64, i64)>,
    total_pct: f32,
    cores_pct: Vec<f32>,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct SysProcPerfInfo {
    idle: i64,
    kernel: i64,
    user: i64,
    dpc: i64,
    interrupt: i64,
    interrupt_count: u32,
    _pad: u32,
}

impl CpuMeter {
    fn sample(&mut self) {
        use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
        type NtQuery = unsafe extern "system" fn(u32, *mut u8, u32, *mut u32) -> i32;
        static FN: OnceLock<Option<NtQuery>> = OnceLock::new();
        let query = *FN.get_or_init(|| unsafe {
            let ntdll = GetModuleHandleA(c"ntdll.dll".as_ptr().cast());
            if ntdll.is_null() {
                return None;
            }
            let p = GetProcAddress(ntdll, c"NtQuerySystemInformation".as_ptr().cast())?;
            Some(std::mem::transmute::<unsafe extern "system" fn() -> isize, NtQuery>(p))
        });
        let Some(query) = query else { return };
        let n = std::thread::available_parallelism().map_or(8, |n| n.get()).min(64);
        let mut buf = vec![SysProcPerfInfo::default(); n];
        let mut ret = 0u32;
        let status = unsafe {
            query(8, buf.as_mut_ptr().cast(), (buf.len() * size_of::<SysProcPerfInfo>()) as u32, &mut ret)
        };
        if status < 0 {
            return;
        }
        let count = (ret as usize / size_of::<SysProcPerfInfo>()).min(buf.len());
        if count == 0 {
            return;
        }
        let now: Vec<(i64, i64)> = buf[..count].iter().map(|p| (p.idle, p.kernel + p.user)).collect();
        if self.prev.len() == count {
            self.cores_pct.clear();
            let mut sum = 0.0f32;
            for (&(pi, pt), &(ni, nt)) in self.prev.iter().zip(&now) {
                let busy = busy_pct((pi, pt), (ni, nt));
                self.cores_pct.push(busy);
                sum += busy;
            }
            self.total_pct = sum / count as f32;
        }
        self.prev = now;
    }
}

#[derive(Clone, Copy, Default)]
struct Counters([u64; COUNTERS]);
const COUNTERS: usize = 11;

impl Counters {
    fn now() -> Counters {
        use super::{CARD_CLOCK_US, COPY_BACK, PREPARING};
        let get = |a: &std::sync::atomic::AtomicU64| a.load(Ordering::Relaxed);
        #[cfg(feature = "diagnostics")]
        let record_ns = crate::copystat::peek(crate::copystat::C::RecordNs);
        #[cfg(not(feature = "diagnostics"))]
        let record_ns = 0;
        Counters([
            super::worked(),
            get(&CARD_CLOCK_US[0]),
            get(&CARD_CLOCK_US[1]),
            get(&CARD_CLOCK_US[2]),
            get(&PREPARING[3]),
            get(&COPY_BACK[0]),
            get(&COPY_BACK[1]),
            get(&crate::shadow::CARD_STATS[0]),
            crate::draw::dropped_now(),
            record_ns,
            crate::draw::drawn(),
        ])
    }

    fn since(self, before: Counters, frames: f64) -> [f64; COUNTERS] {
        let mut out = [0.0; COUNTERS];
        for (i, out) in out.iter_mut().enumerate() {
            let (was, is) = (before.0[i], self.0[i]);
            *out = if is >= was { is - was } else { is } as f64 / frames;
        }
        out
    }
}

pub(super) struct Snapshot {
    pub(super) fps: f64,
    pub(super) frame_ms: f64,
    pub(super) gpu_ms: f64,
    pub(super) gpu_pct: u32,
    pub(super) submit_ms: f64,
    pub(super) low1_fps: f64,
    pub(super) max_ms: f64,
    pub(super) draws: u64,
    pub(super) dropped: u64,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) gpu_name: String,
    pub(super) vram_local: Option<(u64, u64)>,
    pub(super) vram_dedicated: Option<u64>,
    pub(super) vram_shared: Option<u64>,
    pub(super) vram_rt_gb: f64,
    pub(super) vram_tex_gb: f64,
    pub(super) vram_buf_gb: f64,
    pub(super) vram_bvh_gb: f64,
    pub(super) vram_other: Option<u64>,
    pub(super) cpu_name: String,
    pub(super) cpu_pct: f32,
    pub(super) cores_pct: Vec<f32>,
    pub(super) proc_ram_gb: f64,
    pub(super) sys_ram_used_gb: f64,
    pub(super) sys_ram_total_gb: f64,
    pub(super) sys_commit_gb: f64,
    pub(super) inplace_regions: usize,
    pub(super) inplace_mb: u64,
    pub(super) coalesced_barriers: f32,
    pub(super) history_ms: Vec<f32>,
    pub(super) submit_history_ms: Vec<f32>,
}

fn gib(bytes: u64) -> f64 {
    bytes as f64 / (1u64 << 30) as f64
}

fn display_gib(bytes: Option<u64>) -> String {
    bytes.map_or_else(|| "N/A".to_owned(), |n| format!("{:.1}", gib(n)))
}

fn video_memory_lines(
    local: Option<(u64, u64)>,
    dedicated: Option<u64>,
    shared: Option<u64>,
) -> [String; 2] {
    [
        format!(
            "LOCAL {} | BUDGET {} GiB",
            display_gib(local.map(|s| s.0)),
            display_gib(local.map(|s| s.1)),
        ),
        format!("DED CAP {} | SHARED {} GiB", display_gib(dedicated), display_gib(shared)),
    ]
}

pub(super) fn fps_color(fps: f64) -> [u8; 3] {
    if fps >= 28.0 {
        EMERALD
    } else if fps >= 18.0 {
        GOLD
    } else if fps >= 10.0 {
        AMBER
    } else {
        CORAL
    }
}

pub(super) fn frametime_color(ms: f32) -> [u8; 3] {
    if ms <= 35.0 {
        EMERALD
    } else if ms <= 58.0 {
        GOLD
    } else if ms <= 95.0 {
        AMBER
    } else {
        CORAL
    }
}

fn trunc(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_owned() } else { s[..max].to_owned() }
}

fn frame_stats_ms(hist: &[f32]) -> (f32, f32, f32) {
    if hist.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut s: Vec<f32> = hist.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    let min = s[0];
    let med = s[s.len() / 2];
    let p95 = s[(s.len() * 95 / 100).min(s.len() - 1)];
    (min, med, p95)
}

fn paint_basic_hud(s: &Snapshot, cfg: &HudSettings) -> Canvas {
    let mut c = Canvas::new(BASIC_W, BASIC_H, WELL_BG);
    c.rounded_rect_grad_v(0, 0, BASIC_W, BASIC_H, 10.0, CARD_TOP, CARD_BOT);
    c.fill_grad_h(10, 1, BASIC_W - 20, 2, INDIGO, AMBER);
    c.rounded_rect_stroke(0, 0, BASIC_W, BASIC_H, 10.0, BORDER, 1.0);

    // Header
    c.draw_vesper_emblem(12, 7, 18);
    c.text(36, 9, 2, WHITE, "VESPER");
    c.rounded_rect_fill(114, 8, 42, 16, 4.0, [26, 33, 62], 1.0);
    c.rounded_rect_stroke(114, 8, 42, 16, 4.0, INDIGO, 0.8);
    c.text(120, 13, 1, INDIGO, "BASIC");
    c.text_sep(258, 12, 1, MUTED, SEP, "~ Menu | F3 Mode");

    // Hero FPS row
    let fps_col = fps_color(s.fps);
    let fps_txt = format!("{:.1}", s.fps);
    let num_w = draw_hero_numerals(&mut c, 14.0, 34.0, 24.0, fps_col, &fps_txt);
    let info_x = (14.0 + num_w + 6.0).round() as u32;
    c.text(info_x, 35, 1, MUTED, "FPS");
    c.text(info_x, 45, 2, WHITE, &format!("{:.1} ms", s.frame_ms));

    // Right pills: 1% LOW & PEAK
    let max_fps = s.history_ms.iter().copied().fold(f32::INFINITY, f32::min);
    let peak_fps = if max_fps.is_finite() && max_fps > 0.1 {
        1000.0 / f64::from(max_fps)
    } else {
        s.fps
    };
    let pills = [
        (194u32, "1% LOW", format!("{:.1}", s.low1_fps), GOLD),
        (274u32, "PEAK", format!("{:.1}", peak_fps.max(s.fps)), EMERALD),
    ];
    for (px, title, val, col) in pills {
        c.rounded_rect_fill(px, 32, 74, 28, 6.0, WELL_BG, 1.0);
        c.rounded_rect_stroke(px, 32, 74, 28, 6.0, WELL_BORDER, 1.0);
        c.text(px + 8, 36, 1, MUTED, title);
        c.text(px + 8, 47, 1, col, &format!("{val} fps"));
    }

    // Sparkline + MIN/MED/P95 stats column
    c.draw_sparkline(
        12,
        66,
        258,
        46,
        &s.history_ms,
        None,
        cfg.ceiling_ms(),
        cfg.target_fps,
        frametime_color,
    );
    let (min_ms, med_ms, p95_ms) = frame_stats_ms(&s.history_ms);
    c.rounded_rect_fill(276, 66, 72, 46, 6.0, WELL_BG, 1.0);
    c.rounded_rect_stroke(276, 66, 72, 46, 6.0, WELL_BORDER, 1.0);
    c.text(282, 71, 1, EMERALD, &format!("MIN{:>5.1}", min_ms));
    c.text(282, 85, 1, WHITE, &format!("MED{:>5.1}", med_ms));
    c.text(282, 99, 1, AMBER, &format!("P95{:>5.1}", p95_ms));

    // Compact bottom strip (4 mini tiles)
    let vram_used_g = s.vram_local.map_or(0.0, |(u, _)| gib(u));
    let vram_bud_g = s.vram_local.map_or(16.0, |(_, b)| gib(b).max(1.0));
    let tiles = [
        (12u32, format!("GPU {}%", s.gpu_pct), (s.gpu_pct as f32 / 100.0).clamp(0.0, 1.0), VIOLET),
        (98u32, format!("VRAM {:.1}G", vram_used_g), (vram_used_g / vram_bud_g) as f32, CYAN),
        (184u32, format!("RAM {:.1}G", s.proc_ram_gb), (s.proc_ram_gb / s.sys_ram_total_gb.max(1.0)) as f32, AMBER),
        (270u32, format!("{}x{}", s.width, s.height), 1.0, INDIGO),
    ];
    for (tx, txt, fill_t, col) in tiles {
        let tw_box = 78u32;
        c.rounded_rect_fill(tx, 117, tw_box, 17, 4.0, [14, 18, 34], 1.0);
        c.rounded_rect_stroke(tx, 117, tw_box, 17, 4.0, WELL_BORDER, 1.0);
        let tw = text_width(&txt, 1);
        c.text(tx + tw_box.saturating_sub(tw) / 2, 121, 1, WHITE, &txt);
        let bw = (((tw_box - 6) as f32) * fill_t.clamp(0.0, 1.0)).round() as u32;
        if bw > 0 {
            c.rect(tx + 3, 131, bw, 2, col);
        }
    }

    c
}

fn paint_advanced_hud(s: &Snapshot, cfg: &HudSettings) -> Canvas {
    let mut c = Canvas::new(ADV_W, ADV_H, WELL_BG);
    c.rounded_rect_grad_v(0, 0, ADV_W, ADV_H, 12.0, CARD_TOP, CARD_BOT);
    c.fill_grad_h(12, 1, ADV_W - 24, 2, INDIGO, AMBER);
    c.rounded_rect_stroke(0, 0, ADV_W, ADV_H, 12.0, BORDER, 1.0);

    // Header
    c.draw_vesper_emblem(14, 9, 20);
    c.text(42, 12, 2, WHITE, "VESPER");
    c.rounded_rect_fill(120, 11, 58, 16, 4.0, [36, 26, 62], 1.0);
    c.rounded_rect_stroke(120, 11, 58, 16, 4.0, VIOLET, 0.85);
    c.text(125, 16, 1, VIOLET, "ADVANCED");
    c.text_sep(396, 15, 1, MUTED, SEP, "~ Menu | F3 Mode");

    // Hero FPS + 3 breakdown pills
    let fps_col = fps_color(s.fps);
    let fps_txt = format!("{:.1}", s.fps);
    let nw = draw_hero_numerals(&mut c, 14.0, 38.0, 28.0, fps_col, &fps_txt);
    let ix = (14.0 + nw + 8.0).round() as u32;
    c.text(ix, 39, 1, MUTED, "FPS");
    c.text(ix, 50, 2, WHITE, &format!("{:.1} ms", s.frame_ms));

    let hero_pills = [
        (214u32, "GPU TIME", format!("{:.1} ms", s.gpu_ms), VIOLET),
        (308u32, "SUBMIT/PREP", format!("{:.1} ms", s.submit_ms), CYAN),
        (402u32, "1% LOW / MAX", format!("{:.0}f / {:.0}m", s.low1_fps, s.max_ms), GOLD),
    ];
    for (px, title, val, col) in hero_pills {
        c.rounded_rect_fill(px, 37, 84, 30, 6.0, WELL_BG, 1.0);
        c.rounded_rect_stroke(px, 37, 84, 30, 6.0, WELL_BORDER, 1.0);
        c.text(px + 7, 41, 1, MUTED, title);
        c.text(px + 7, 53, 1, col, &val);
    }

    // Dual-series smooth frame-time graph
    c.draw_sparkline(
        14,
        74,
        ADV_W - 28,
        96,
        &s.history_ms,
        Some(&s.submit_history_ms),
        cfg.ceiling_ms(),
        cfg.target_fps,
        frametime_color,
    );
    let legend = format!("FRAME  |  SUBMIT  |  0-{:.0}ms", cfg.ceiling_ms());
    let lw = text_width(&legend, 1);
    c.text_sep(ADV_W - 22 - lw, 80, 1, MUTED, SEP, &legend);
    c.rect(ADV_W - 22 - lw - 8, 82, 5, 3, EMERALD);

    // Pipeline & Sync telemetry card
    c.rounded_rect_fill(14, 178, ADV_W - 28, 48, 8.0, WELL_BG, 1.0);
    c.rounded_rect_stroke(14, 178, ADV_W - 28, 48, 8.0, WELL_BORDER, 1.0);
    let tear_s = if crate::live_controls::tearing_enabled() { "TEAR ON" } else { "VSYNC" };
    let clamp_s = if crate::draw::live_indirect_clamp() { "CLAMP ON" } else { "CLAMP OFF" };
    let cols = [
        (24u32, "DRAWS / DROPPED", format!("{} / {}", s.draws, s.dropped), WHITE),
        (162u32, "IN-PLACE HOT", format!("{} rgn ({} MB)", s.inplace_regions, s.inplace_mb), EMERALD),
        (292u32, "COALESCED", format!("{:.0} / frame", s.coalesced_barriers), INDIGO),
        (398u32, "RUNTIME", tear_s.to_owned(), if crate::draw::live_indirect_clamp() { CYAN } else { CORAL }),
    ];
    for (cx, lbl, val, col) in cols {
        c.text(cx, 186, 1, MUTED, lbl);
        c.text(cx, 200, 1, col, &val);
    }
    c.text(398, 212, 1, if crate::draw::live_indirect_clamp() { EMERALD } else { CORAL }, clamp_s);

    // GPU & VRAM card
    c.rounded_rect_fill(14, 234, ADV_W - 28, 92, 8.0, WELL_BG, 1.0);
    c.rounded_rect_stroke(14, 234, ADV_W - 28, 92, 8.0, WELL_BORDER, 1.0);
    c.text(24, 242, 1, INDIGO, "GPU");
    let gpu_line = format!("{}  ({}% busy | {}x{})", trunc(&s.gpu_name, 34), s.gpu_pct, s.width, s.height);
    c.text(62, 242, 1, WHITE, &gpu_line);

    // Stacked VRAM bar
    let bar_x = 24u32;
    let bar_w = ADV_W - 48;
    c.rounded_rect_fill(bar_x, 258, bar_w, 8, 4.0, BAR_BG, 1.0);
    let other_g = s.vram_other.map_or(0.0, gib);
    let budget_g = s.vram_local.map_or(16.0, |(_, b)| gib(b).max(1.0));
    let segs = [
        (s.vram_rt_gb, VIOLET),
        (s.vram_tex_gb, CYAN),
        (s.vram_buf_gb, AMBER),
        (s.vram_bvh_gb, EMERALD),
        (other_g, SEP),
    ];
    let mut sx = bar_x;
    for (gb, col) in segs {
        let sw = ((gb / budget_g) * bar_w as f64).round() as u32;
        let clamped = sw.min(bar_x + bar_w - sx);
        if clamped > 0 {
            c.rect(sx, 259, clamped, 6, col);
            sx += clamped;
        }
    }

    let [local_line, cap_line] = video_memory_lines(s.vram_local, s.vram_dedicated, s.vram_shared);
    c.text_sep(24, 274, 1, WHITE, SEP, &local_line);
    c.text_sep(24, 289, 1, MUTED, SEP, &cap_line);
    let vram_detail = format!(
        "RT {:.1}G | TEX {:.1}G | BUF {:.1}G | BVH {:.1}G | OTH {}",
        s.vram_rt_gb, s.vram_tex_gb, s.vram_buf_gb, s.vram_bvh_gb, display_gib(s.vram_other)
    );
    c.text_sep(24, 306, 1, MUTED, SEP, &vram_detail);

    // CPU & System Memory card
    c.rounded_rect_fill(14, 334, ADV_W - 28, 70, 8.0, WELL_BG, 1.0);
    c.rounded_rect_stroke(14, 334, ADV_W - 28, 70, 8.0, WELL_BORDER, 1.0);
    c.text(24, 342, 1, INDIGO, "CPU");
    c.text(62, 342, 1, WHITE, &format!("{}  ({:.0}%)", trunc(&s.cpu_name, 38), s.cpu_pct));

    // Per-core equalizer bars
    let cores = s.cores_pct.len().clamp(1, 32);
    let area_w = ADV_W - 86;
    let gap = 2u32;
    let cw = ((area_w + gap) / cores as u32).saturating_sub(gap).max(4);
    let bh = 18u32;
    let by = 356u32;
    for (i, &pct) in s.cores_pct.iter().take(cores).enumerate() {
        let bx = 62 + i as u32 * (cw + gap);
        c.rect(bx, by, cw, bh, BAR_BG);
        let fh = ((pct / 100.0) * bh as f32).round() as u32;
        if fh > 0 {
            let fh = fh.min(bh);
            let col = if pct >= 85.0 { AMBER } else if pct >= 50.0 { VIOLET } else { CYAN };
            c.rect(bx, by + bh - fh, cw, fh, col);
        }
    }

    c.text(24, 383, 1, AMBER, "MEM");
    let mem_line = format!(
        "PROC {:.1} GiB | SYS {:.1}/{:.1} GiB | CMT {:.1} GiB",
        s.proc_ram_gb, s.sys_ram_used_gb, s.sys_ram_total_gb, s.sys_commit_gb
    );
    c.text_sep(62, 383, 1, WHITE, SEP, &trunc(&mem_line, 68));

    c
}

/// GPU upload ring + card copy target for a single overlay surface.
struct Buffers {
    format: DXGI_FORMAT,
    scale: u32,
    w: u32,
    h: u32,
    pitch: u32,
    uploads: Vec<(ID3D12Resource, *mut u8)>,
    card: ID3D12Resource,
    turn: usize,
    fresh: bool,
    uploaded: bool,
}

// SAFETY: free-threaded resources updated only under the presenter's lock.
unsafe impl Send for Buffers {}

impl Buffers {
    fn new(host: &Host, format: DXGI_FORMAT, scale: u32, (logical_w, logical_h): (u32, u32)) -> Option<Buffers> {
        let w = logical_w * scale;
        let h = logical_h * scale;
        let pitch = (w * bpp(format)? as u32).next_multiple_of(256);
        let bytes = u64::from(pitch) * u64::from(h);
        let uploads = (0..UPLOADS).map(|_| host.upload_buffer(bytes)).collect::<Option<Vec<_>>>()?;
        let heap = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_DEFAULT,
            ..Default::default()
        };
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: bytes,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            ..Default::default()
        };
        let mut card: Option<ID3D12Resource> = None;
        // SAFETY: plain committed default buffer creation.
        unsafe {
            host.device
                .CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &desc, D3D12_RESOURCE_STATE_COMMON, None, &mut card)
                .ok()?;
        }
        Some(Buffers { format, scale, w, h, pitch, uploads, card: card?, turn: 0, fresh: false, uploaded: false })
    }

    fn write(&mut self, canvas: &Canvas) {
        let picture = canvas.pack(self.scale, self.format, self.pitch as usize);
        self.turn = (self.turn + 1) % UPLOADS;
        // SAFETY: mapped upload buffer sized for `picture.len()`, card uses previous turn slots.
        unsafe { std::ptr::copy_nonoverlapping(picture.as_ptr(), self.uploads[self.turn].1, picture.len()) };
        self.fresh = true;
    }

    unsafe fn record_copy(
        &mut self,
        list: &ID3D12GraphicsCommandList,
        back: &ID3D12Resource,
        (dst_x, dst_y): (u32, u32),
        (win_w, win_h): (u32, u32),
    ) {
        if dst_x >= win_w || dst_y >= win_h {
            return;
        }
        unsafe {
            if std::mem::take(&mut self.fresh) {
                if self.uploaded {
                    list.ResourceBarrier(&[transition(
                        &self.card,
                        D3D12_RESOURCE_STATE_COPY_SOURCE,
                        D3D12_RESOURCE_STATE_COPY_DEST,
                    )]);
                }
                list.CopyResource(&self.card, &self.uploads[self.turn].0);
                list.ResourceBarrier(&[transition(
                    &self.card,
                    D3D12_RESOURCE_STATE_COPY_DEST,
                    D3D12_RESOURCE_STATE_COPY_SOURCE,
                )]);
                self.uploaded = true;
            }
            let from = D3D12_TEXTURE_COPY_LOCATION {
                pResource: std::mem::transmute_copy(&self.card),
                Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                        Offset: 0,
                        Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                            Format: self.format,
                            Width: self.w,
                            Height: self.h,
                            Depth: 1,
                            RowPitch: self.pitch,
                        },
                    },
                },
            };
            let to = D3D12_TEXTURE_COPY_LOCATION {
                pResource: std::mem::transmute_copy(back),
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { SubresourceIndex: 0 },
            };
            let part = D3D12_BOX {
                left: 0,
                top: 0,
                front: 0,
                right: self.w.min(win_w - dst_x),
                bottom: self.h.min(win_h - dst_y),
                back: 1,
            };
            if part.right > 0 && part.bottom > 0 {
                list.CopyTextureRegion(&to, dst_x, dst_y, 0, &from, Some(&part));
            }
        }
    }
}

/// Presenter HUD & interactive Vesper control menu controller.
pub(super) struct Panel {
    since: Instant,
    frames: u32,
    last: Instant,
    worst: f64,
    before: Counters,
    history: VecDeque<f32>,
    submit_history: VecDeque<f32>,
    cpu: CpuMeter,
    last_size: (u32, u32),
    snap: Option<Snapshot>,
    settings: HudSettings,
    menu: MenuState,
    hud_dirty: bool,
    menu_dirty: bool,
    hud_buffers: Option<Buffers>,
    menu_buffers: Option<Buffers>,
    hud_failed: bool,
    menu_failed: bool,
}

impl Panel {
    pub(super) fn new() -> Panel {
        let now = Instant::now();
        Panel {
            since: now,
            frames: 0,
            last: now,
            worst: 0.0,
            before: Counters::default(),
            history: VecDeque::with_capacity(HISTORY),
            submit_history: VecDeque::with_capacity(HISTORY),
            cpu: CpuMeter::default(),
            last_size: (1920, 1080),
            snap: None,
            settings: HudSettings::default(),
            menu: MenuState::new(),
            hud_dirty: false,
            menu_dirty: false,
            hud_buffers: None,
            menu_buffers: None,
            hud_failed: false,
            menu_failed: false,
        }
    }

    /// Updates frame timing, polls `F3` (HUD cycle) and `~` (Vesper menu).
    pub(super) fn frame(&mut self, hwnd: isize, line: bool) {
        if f3_pressed() {
            self.settings.mode = match self.settings.mode {
                HudMode::Off => HudMode::Basic,
                HudMode::Basic => HudMode::Advanced,
                HudMode::Advanced => HudMode::Off,
            };
            self.hud_dirty = true;
        }
        let scale = self.settings.effective_scale(self.last_size.1);
        if self.menu.poll(hwnd, self.last_size, scale, &mut self.settings) {
            self.menu_dirty = true;
            self.hud_dirty = true;
        }

        let need_sample = self.settings.mode != HudMode::Off || self.menu.open;
        if !need_sample {
            self.frames = 0;
            return;
        }

        let now = Instant::now();
        let dt_ms = (now - self.last).as_secs_f64() * 1e3;
        if self.frames == 0 && now - self.last > 4 * EVERY {
            (self.since, self.last, self.worst, self.before) = (now, now, 0.0, Counters::now());
            self.frames = 0;
            self.cpu.sample();
        } else {
            if self.history.len() == HISTORY {
                self.history.pop_front();
            }
            self.history.push_back(dt_ms as f32);
            self.worst = self.worst.max(dt_ms);
            self.last = now;
        }
        self.frames += 1;

        let elapsed = now - self.since;
        if elapsed < EVERY && !(line && elapsed * 2 >= EVERY) {
            return;
        }
        let Some(host) = super::host() else { return };
        self.cpu.sample();
        let frames = f64::from(self.frames);
        let counters = Counters::now();
        let c = counters.since(self.before, frames);
        self.before = counters;

        let ms = elapsed.as_secs_f64() * 1e3 / frames;
        let fps = 1e3 / ms.max(0.001);
        let idle = (c[1] / 1e3).min(ms);
        let gpu_ms = (ms - idle).max(0.0);
        let gpu_pct = ((gpu_ms / ms.max(0.001)) * 100.0).clamp(0.0, 100.0).round() as u32;
        let submit_ms = c[0] / 1e3;
        if self.submit_history.len() == HISTORY {
            self.submit_history.pop_front();
        }
        self.submit_history.push_back(submit_ms as f32);

        let low1_fps = if !self.history.is_empty() {
            let mut sorted: Vec<f32> = self.history.iter().copied().collect();
            sorted.sort_by(|a, b| a.total_cmp(b));
            let idx = (sorted.len() * 99 / 100).min(sorted.len() - 1);
            1e3 / f64::from(sorted[idx]).max(0.001)
        } else {
            fps
        };

        let private = process_private_bytes();
        let [local, shared] = host.sample_video_memory();
        let all_tex = crate::resource::HOST_TEXTURES[1].load(Ordering::Relaxed);
        let filled_tex = crate::resource::KINDS[1].load(Ordering::Relaxed);
        let rt_bytes = all_tex.saturating_sub(filled_tex);
        let pool_bytes = crate::pool::memory()[0] << 20;
        let tex_bytes = filled_tex.max(pool_bytes) + crate::tiles::given_total();
        let [buf_bytes, bvh_bytes] = crate::shadow::vram_breakdown();
        let known = rt_bytes + tex_bytes + buf_bytes + bvh_bytes;
        let other = local.map(|s| s.CurrentUsage.saturating_sub(known));
        let (sys_used, sys_total, sys_commit, _) = system_memory();
        let (gpu_name, dedicated) = gpu_info(host);
        let (inplace_regions, inplace_mb) = crate::port::live_stats();

        self.snap = Some(Snapshot {
            fps,
            frame_ms: ms,
            gpu_ms,
            gpu_pct,
            submit_ms,
            low1_fps,
            max_ms: self.worst,
            draws: c[10].round() as u64,
            dropped: c[8].round() as u64,
            width: self.last_size.0,
            height: self.last_size.1,
            gpu_name,
            vram_local: local.map(|s| (s.CurrentUsage, s.Budget)),
            vram_dedicated: dedicated,
            vram_shared: shared.map(|s| s.CurrentUsage),
            vram_rt_gb: gib(rt_bytes),
            vram_tex_gb: gib(tex_bytes),
            vram_buf_gb: gib(buf_bytes),
            vram_bvh_gb: gib(bvh_bytes),
            vram_other: other,
            cpu_name: cpu_name().to_owned(),
            cpu_pct: self.cpu.total_pct,
            cores_pct: self.cpu.cores_pct.clone(),
            proc_ram_gb: gib(private),
            sys_ram_used_gb: gib(sys_used),
            sys_ram_total_gb: gib(sys_total),
            sys_commit_gb: gib(sys_commit),
            inplace_regions,
            inplace_mb,
            coalesced_barriers: super::rec::last_coalesced(),
            history_ms: self.history.iter().copied().collect(),
            submit_history_ms: self.submit_history.iter().copied().collect(),
        });
        self.hud_dirty = true;
        if self.menu.open {
            self.menu_dirty = true;
        }
        (self.since, self.frames, self.worst) = (now, 0, 0.0);
    }

    pub(super) fn draw(
        &mut self,
        host: &Host,
        list: &HostList,
        back: &ID3D12Resource,
        (width, height): (u32, u32),
        format: DXGI_FORMAT,
    ) {
        self.last_size = (width, height);
        let scale = self.settings.effective_scale(height);

        #[cfg(feature = "diagnostics")]
        let _ours = crate::replay::record::unrecorded();

        // 1. HUD overlay
        if self.settings.mode != HudMode::Off
            && !self.hud_failed
            && let Some(snap) = self.snap.as_ref()
        {
            let dims = self.settings.dims();
            if self.hud_buffers.as_ref().is_none_or(|b| {
                (b.format, b.scale, b.w, b.h) != (format, scale, dims.0 * scale, dims.1 * scale)
            }) {
                std::mem::forget(self.hud_buffers.take());
                self.hud_buffers = Buffers::new(host, format, scale, dims);
                self.hud_failed = self.hud_buffers.is_none();
                self.hud_dirty = true;
            }
            if let Some(buf) = &mut self.hud_buffers {
                if std::mem::take(&mut self.hud_dirty) {
                    let canvas = match self.settings.mode {
                        HudMode::Advanced => paint_advanced_hud(snap, &self.settings),
                        HudMode::Basic | HudMode::Off => paint_basic_hud(snap, &self.settings),
                    };
                    buf.write(&canvas);
                }
                let margin = 16 * scale;
                let dst_x = match self.settings.corner {
                    HudCorner::TopLeft | HudCorner::BottomLeft => margin,
                    HudCorner::TopRight | HudCorner::BottomRight => width.saturating_sub(buf.w + margin),
                };
                let dst_y = match self.settings.corner {
                    HudCorner::TopLeft | HudCorner::TopRight => margin,
                    HudCorner::BottomLeft | HudCorner::BottomRight => height.saturating_sub(buf.h + margin),
                };
                // SAFETY: open copy list with valid backbuffer and card buffer.
                unsafe { buf.record_copy(&list.list, back, (dst_x, dst_y), (width, height)) };
            }
        }

        // 2. Centered Vesper menu (zero GPU uploads when closed)
        if self.menu.open && !self.menu_failed {
            if self.menu_buffers.as_ref().is_none_or(|b| (b.format, b.scale) != (format, scale)) {
                std::mem::forget(self.menu_buffers.take());
                self.menu_buffers = Buffers::new(host, format, scale, (MENU_W, MENU_H));
                self.menu_failed = self.menu_buffers.is_none();
                self.menu_dirty = true;
            }
            if let Some(buf) = &mut self.menu_buffers {
                if std::mem::take(&mut self.menu_dirty) {
                    let canvas = paint_menu(&self.menu, &self.settings, self.snap.as_ref());
                    buf.write(&canvas);
                }
                let origin = menu_origin((width, height), scale);
                // SAFETY: open copy list with valid backbuffer and card buffer.
                unsafe { buf.record_copy(&list.list, back, origin, (width, height)) };
            }
        }
    }
}

fn busy_pct((pi, pt): (i64, i64), (ni, nt): (i64, i64)) -> f32 {
    let span = nt - pt;
    if span <= 0 {
        return 0.0;
    }
    let di = (ni - pi).clamp(0, span) as f32;
    ((1.0 - di / span as f32) * 100.0).clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::draw2d::encode;
    use super::font::glyph;
    use super::*;

    fn sample_snapshot() -> Snapshot {
        Snapshot {
            fps: 22.8,
            frame_ms: 43.9,
            gpu_ms: 34.2,
            gpu_pct: 78,
            submit_ms: 19.8,
            low1_fps: 15.2,
            max_ms: 68.4,
            draws: 3984,
            dropped: 435,
            width: 1920,
            height: 1080,
            gpu_name: "AMD Radeon RX 9070 XT".into(),
            vram_local: Some((10_800 << 20, 14_023 << 20)),
            vram_dedicated: Some(16 << 30),
            vram_shared: Some(8 << 30),
            vram_rt_gb: 2.8,
            vram_tex_gb: 3.5,
            vram_buf_gb: 1.2,
            vram_bvh_gb: 0.8,
            vram_other: Some(2_200 << 20),
            cpu_name: "AMD Ryzen 7 5800X3D".into(),
            cpu_pct: 36.0,
            cores_pct: vec![78.0, 42.0, 91.0, 24.0, 64.0, 35.0, 18.0, 52.0, 88.0, 44.0, 29.0, 61.0, 33.0, 49.0, 22.0, 15.0],
            proc_ram_gb: 26.8,
            sys_ram_used_gb: 24.9,
            sys_ram_total_gb: 31.9,
            sys_commit_gb: 30.2,
            inplace_regions: 22,
            inplace_mb: 348,
            coalesced_barriers: 119.0,
            history_ms: (0..90)
                .map(|i| 40.0 + ((i as f32 * 0.23).sin() * 8.0) + if i == 62 { 24.0 } else { 0.0 })
                .collect(),
            submit_history_ms: (0..90)
                .map(|i| 19.0 + ((i as f32 * 0.21).cos() * 4.5))
                .collect(),
        }
    }

    fn write_png(path: &std::path::Path, canvas: &Canvas) {
        fn crc32(data: &[u8]) -> u32 {
            let mut crc = 0xFFFF_FFFFu32;
            for &b in data {
                crc ^= u32::from(b);
                for _ in 0..8 {
                    crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
                }
            }
            !crc
        }
        fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], payload: &[u8]) {
            out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            out.extend_from_slice(tag);
            out.extend_from_slice(payload);
            let mut buf = Vec::with_capacity(4 + payload.len());
            buf.extend_from_slice(tag);
            buf.extend_from_slice(payload);
            out.extend_from_slice(&crc32(&buf).to_be_bytes());
        }
        let w = canvas.w as u32;
        let h = canvas.h as u32;
        let mut raw = Vec::with_capacity((w as usize * 3 + 1) * h as usize);
        for y in 0..canvas.h {
            raw.push(0);
            for x in 0..canvas.w {
                raw.extend_from_slice(&canvas.rgb[y * canvas.w + x]);
            }
        }
        // Uncompressed zlib wrapper (no external deps needed).
        let mut z = vec![0x78u8, 0x01];
        for (i, blk) in raw.chunks(65535).enumerate() {
            let last = (i + 1) * 65535 >= raw.len();
            z.push(u8::from(last));
            let len = blk.len() as u16;
            z.extend_from_slice(&len.to_le_bytes());
            z.extend_from_slice(&(!len).to_le_bytes());
            z.extend_from_slice(blk);
        }
        let (mut s1, mut s2) = (1u32, 0u32);
        for &b in &raw {
            s1 = (s1 + u32::from(b)) % 65521;
            s2 = (s2 + s1) % 65521;
        }
        z.extend_from_slice(&((s2 << 16) | s1).to_be_bytes());

        let mut png = Vec::from(*b"\x89PNG\r\n\x1a\n");
        let mut ihdr = Vec::with_capacity(13);
        ihdr.extend_from_slice(&w.to_be_bytes());
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        chunk(&mut png, b"IHDR", &ihdr);
        chunk(&mut png, b"IDAT", &z);
        chunk(&mut png, b"IEND", &[]);
        std::fs::write(path, png).unwrap();
    }

    #[test]
    fn characters_differ() {
        let used: Vec<u8> = (b'0'..=b'9').chain(b'A'..=b'Z').chain(b'a'..=b'z').chain(*b".,/-+|%:()[]~!<>=_*& ").collect();
        for (i, a) in used.iter().enumerate() {
            assert_ne!(glyph(*a), glyph(b'#'), "{} has a picture", *a as char);
            assert!(glyph(*a).iter().all(|row| row >> font::GLYPH.0 == 0));
            for b in &used[i + 1..] {
                assert_ne!(glyph(*a), glyph(*b), "{} {}", *a as char, *b as char);
            }
        }
    }

    #[test]
    fn video_memory_keeps_budget_capacity_and_usage_distinct() {
        let lines = video_memory_lines(Some((17 << 30, 16 << 30)), Some(24 << 30), Some(1 << 29));
        assert_eq!(lines, ["LOCAL 17.0 | BUDGET 16.0 GiB", "DED CAP 24.0 | SHARED 0.5 GiB"]);
    }

    #[test]
    fn missing_video_memory_samples_stay_unknown() {
        assert_eq!(
            video_memory_lines(None, Some(0), Some(0)),
            ["LOCAL N/A | BUDGET N/A GiB", "DED CAP 0.0 | SHARED 0.0 GiB"],
        );
        assert_eq!(
            video_memory_lines(Some((0, 1 << 30)), None, None),
            ["LOCAL 0.0 | BUDGET 1.0 GiB", "DED CAP N/A | SHARED N/A GiB"],
        );
    }

    #[test]
    fn hud_and_menu_surfaces_pack_cleanly() {
        let snap = sample_snapshot();
        let cfg = HudSettings::default();
        let basic = paint_basic_hud(&snap, &cfg);
        let adv = paint_advanced_hud(&snap, &cfg);
        let mut m = MenuState::new();
        m.open = true;
        let menu_ov = paint_menu(&m, &cfg, Some(&snap));
        m.tab = menu::MenuTab::Lighting;
        let menu_lt = paint_menu(&m, &cfg, Some(&snap));
        m.tab = menu::MenuTab::Lab;
        let menu_lb = paint_menu(&m, &cfg, Some(&snap));

        for canvas in [&basic, &adv, &menu_ov, &menu_lt, &menu_lb] {
            let pitch = (canvas.w as u32 * 4).next_multiple_of(256) as usize;
            let packed = canvas.pack(1, DXGI_FORMAT_R10G10B10A2_UNORM, pitch);
            assert_eq!(packed.len(), pitch * canvas.h);
            let mut corner = [0u8; 4];
            encode(DXGI_FORMAT_R10G10B10A2_UNORM, WELL_BG, &mut corner);
            assert_eq!(&packed[0..4], &corner);
        }

        if let Some(dir) = std::env::var_os("VESPER_DUMP_UI_PREVIEW") {
            let dir = std::path::PathBuf::from(dir);
            let _ = std::fs::create_dir_all(&dir);
            write_png(&dir.join("hud_basic.png"), &basic);
            write_png(&dir.join("hud_advanced.png"), &adv);
            write_png(&dir.join("menu_overlay.png"), &menu_ov);
            write_png(&dir.join("menu_lighting.png"), &menu_lt);
            write_png(&dir.join("menu_lab.png"), &menu_lb);
        }
    }

    #[test]
    fn counters_that_went_back_to_nothing_count_from_there() {
        let (mut a, mut b) = (Counters::default(), Counters::default());
        (a.0[0], b.0[0]) = (10, 30);
        (a.0[1], b.0[1]) = (50, 20);
        let c = b.since(a, 10.0);
        assert_eq!((c[0], c[1]), (2.0, 2.0));
    }
}

#[cfg(test)]
mod busy_tests {
    use super::busy_pct;

    #[test]
    fn busy_is_the_share_of_time_not_idle() {
        assert_eq!(busy_pct((0, 0), (25, 100)), 75.0);
        assert_eq!(busy_pct((0, 0), (100, 100)), 0.0);
    }

    #[test]
    fn counters_that_step_back_do_not_panic() {
        assert_eq!(busy_pct((10, 156_250), (10, 0)), 0.0);
        assert_eq!(busy_pct((500, 1_000), (400, 900)), 0.0);
        assert_eq!(busy_pct((500, 1_000), (400, 1_100)), 100.0);
    }
}
