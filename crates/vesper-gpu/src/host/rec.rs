//! A host list's D3D12 list, with barrier calls joined
//! (`VESPER_COALESCE_BARRIERS=1`; off by default; A/B-able with
//! `VESPER_AB=<s>,VESPER_COALESCE_BARRIERS`).
//!
//! The state changes the host asks for (`crate::list::ensure_state`, the
//! aliasing before a texture's first upload) and the title's own barriers
//! wait here instead of going to the list at once; they go, in the order
//! they were asked for, in one call, at the next use of the list for
//! anything else. Every use goes through [`Rec`]'s `Deref`, so nothing is
//! recorded before a barrier asked for earlier. In a replay of capture
//! replay5 joining the calls that had no work between them saved 2.4 ms of
//! 79.5 (2,490 calls joined, 1,619 of them a texture's state change after
//! its upload followed by the next texture's aliasing).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

use windows::Win32::Graphics::Direct3D12::*;
use windows::core::Interface;

/// Whether barriers are held now (`VESPER_COALESCE_BARRIERS=1`, or its phase
/// of `VESPER_AB`).
pub(crate) fn on() -> bool {
    #[cfg(feature = "diagnostics")]
    if let Some(on) = crate::ab::pick("VESPER_COALESCE_BARRIERS") {
        return on;
    }
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VESPER_COALESCE_BARRIERS").is_some_and(|v| v == "1"))
}

/// Barriers held. Their resources are borrowed: the list's states hold the
/// textures (`HostList::transition`), and the title's barriers name what
/// the title keeps alive until the list has run.
struct Held {
    legacy: Vec<D3D12_RESOURCE_BARRIER>,
    global: Option<D3D12_GLOBAL_BARRIER>,
}

// SAFETY: borrowed interface pointers of free-threaded objects.
unsafe impl Send for Held {}

/// Since the last frame-rate line: barrier calls asked for while held, and
/// calls made of them.
pub(crate) static STATS: [std::sync::atomic::AtomicU64; 2] = [const { std::sync::atomic::AtomicU64::new(0) }; 2];

/// Since the last frame-rate line: enhanced barrier groups `[skipped, enhanced_requested, legacy_uav, coalesced]`.
pub(crate) static GROUP_STATS: [std::sync::atomic::AtomicU64; 4] = [const { std::sync::atomic::AtomicU64::new(0) }; 4];
static LAST_COALESCED_X10: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Coalesced enhanced barriers per frame from the most recent logging window.
pub(crate) fn last_coalesced() -> f32 {
    LAST_COALESCED_X10.load(Relaxed) as f32 / 10.0
}

pub(crate) struct Rec {
    list: ID3D12GraphicsCommandList,
    list7: Option<ID3D12GraphicsCommandList7>,
    any: AtomicBool,
    held: Mutex<Held>,
}

impl Rec {
    pub(crate) fn new(list: ID3D12GraphicsCommandList) -> Self {
        let list7 = Interface::cast::<ID3D12GraphicsCommandList7>(&list).ok();
        Self {
            list,
            list7,
            any: AtomicBool::new(false),
            held: Mutex::new(Held { legacy: Vec::new(), global: None }),
        }
    }

    /// Records `barriers` now, or holds them for the next use of the list.
    /// Consecutive global `UAV(NULL)` barriers in the same batch are deduplicated.
    ///
    /// # Safety
    /// As `ResourceBarrier`'s: an open list, live resources.
    pub(crate) unsafe fn barriers(&self, barriers: &[D3D12_RESOURCE_BARRIER]) {
        if barriers.is_empty() {
            return;
        }
        if !on() {
            // (What was held while it was on goes first.)
            if self.any.load(Relaxed) {
                self.flush();
            }
            // SAFETY: the caller's.
            unsafe { self.list.ResourceBarrier(barriers) };
            return;
        }
        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(g) = held.global.take()
            && let Some(list7) = &self.list7
        {
            // SAFETY: open ID3D12GraphicsCommandList7; preserves order before new legacy barriers.
            unsafe { emit_global(list7, g) };
        }
        // A resource whose state changes again: what is held goes first, so
        // that no call holds two changes of one resource.
        if barriers.iter().filter_map(named).any(|r| held.legacy.iter().filter_map(named).any(|h| h == r)) {
            // SAFETY: as in `flush`.
            unsafe { self.list.ResourceBarrier(&held.legacy) };
            held.legacy.clear();
            STATS[1].fetch_add(1, Relaxed);
        }
        // SAFETY: a plain copy of the descriptions; nothing is released
        // through them (their pointers are borrowed, `ManuallyDrop`).
        let mut has_global_uav = held.legacy.iter().any(is_global_uav);
        for b in barriers {
            if is_global_uav(b) {
                if has_global_uav {
                    continue;
                }
                has_global_uav = true;
            }
            held.legacy.push(unsafe { std::ptr::read(b) });
        }
        STATS[0].fetch_add(1, Relaxed);
        self.any.store(true, Relaxed);
    }

    /// Emits or coalesces an enhanced `D3D12_GLOBAL_BARRIER`. Returns `false` if
    /// the host list does not support `ID3D12GraphicsCommandList7`.
    ///
    /// # Safety
    /// An open list.
    pub(crate) unsafe fn enhanced_global(&self, barrier: D3D12_GLOBAL_BARRIER) -> bool {
        let Some(list7) = &self.list7 else {
            return false;
        };
        if !on() {
            if self.any.load(Relaxed) {
                self.flush();
            }
            // SAFETY: open `ID3D12GraphicsCommandList7` with a valid global barrier group.
            unsafe { emit_global(list7, barrier) };
            return true;
        }
        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        if !held.legacy.is_empty() {
            // SAFETY: open list with live legacy barriers held prior to this enhanced barrier.
            unsafe { self.list.ResourceBarrier(&held.legacy) };
            held.legacy.clear();
            STATS[1].fetch_add(1, Relaxed);
        }
        if let Some(prev) = held.global {
            held.global = Some(merge_global(prev, barrier));
            GROUP_STATS[3].fetch_add(1, Relaxed);
        } else {
            held.global = Some(barrier);
        }
        self.any.store(true, Relaxed);
        true
    }

    /// What is held goes to the list, in one call.
    fn flush(&self) {
        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        self.any.store(false, Relaxed);
        if !held.legacy.is_empty() {
            // SAFETY: the barriers were asked for on this open list, in order;
            // their resources are alive (`Held`).
            unsafe { self.list.ResourceBarrier(&held.legacy) };
            held.legacy.clear();
            STATS[1].fetch_add(1, Relaxed);
        }
        if let Some(g) = held.global.take()
            && let Some(list7) = &self.list7
        {
            // SAFETY: open `ID3D12GraphicsCommandList7`.
            unsafe { emit_global(list7, g) };
        }
    }
}

unsafe fn emit_global(list7: &ID3D12GraphicsCommandList7, barrier: D3D12_GLOBAL_BARRIER) {
    let group = D3D12_BARRIER_GROUP {
        Type: D3D12_BARRIER_TYPE_GLOBAL,
        NumBarriers: 1,
        Anonymous: D3D12_BARRIER_GROUP_0 {
            pGlobalBarriers: &barrier,
        },
    };
    // SAFETY: caller guarantees an open list.
    unsafe { list7.Barrier(&[group]) };
}

fn merge_global(a: D3D12_GLOBAL_BARRIER, b: D3D12_GLOBAL_BARRIER) -> D3D12_GLOBAL_BARRIER {
    const LAYOUT_EXCLUSIVE: i32 = 0x8 | 0x20 | 0x40 | 0x1000 | 0x2000 | 0x10000;
    let access_before = if a.AccessBefore == D3D12_BARRIER_ACCESS_COMMON || b.AccessBefore == D3D12_BARRIER_ACCESS_COMMON {
        D3D12_BARRIER_ACCESS_COMMON
    } else {
        D3D12_BARRIER_ACCESS(a.AccessBefore.0 | b.AccessBefore.0)
    };
    let access_after = if a.AccessAfter == D3D12_BARRIER_ACCESS_COMMON || b.AccessAfter == D3D12_BARRIER_ACCESS_COMMON {
        D3D12_BARRIER_ACCESS_COMMON
    } else {
        D3D12_BARRIER_ACCESS(a.AccessAfter.0 | b.AccessAfter.0)
    };
    let layout_conflict = access_before != D3D12_BARRIER_ACCESS_COMMON
        && access_after != D3D12_BARRIER_ACCESS_COMMON
        && ((access_before.0 | access_after.0) & LAYOUT_EXCLUSIVE != 0)
        && access_before != access_after;
    if access_before == D3D12_BARRIER_ACCESS_COMMON
        || layout_conflict
        || (access_after == D3D12_BARRIER_ACCESS_COMMON && (access_before.0 & LAYOUT_EXCLUSIVE != 0))
    {
        return D3D12_GLOBAL_BARRIER {
            SyncBefore: D3D12_BARRIER_SYNC_ALL,
            SyncAfter: D3D12_BARRIER_SYNC_ALL,
            AccessBefore: D3D12_BARRIER_ACCESS_COMMON,
            AccessAfter: D3D12_BARRIER_ACCESS_COMMON,
        };
    }
    let raw_sb = a.SyncBefore.0 | b.SyncBefore.0;
    let sync_before = if (raw_sb & D3D12_BARRIER_SYNC_ALL.0) != 0 {
        D3D12_BARRIER_SYNC_ALL
    } else {
        D3D12_BARRIER_SYNC(raw_sb)
    };
    let raw_sa = a.SyncAfter.0 | b.SyncAfter.0;
    let sync_after = if access_after == D3D12_BARRIER_ACCESS_COMMON || (raw_sa & D3D12_BARRIER_SYNC_ALL.0) != 0 {
        D3D12_BARRIER_SYNC_ALL
    } else {
        D3D12_BARRIER_SYNC(raw_sa)
    };
    D3D12_GLOBAL_BARRIER {
        SyncBefore: sync_before,
        SyncAfter: sync_after,
        AccessBefore: access_before,
        AccessAfter: access_after,
    }
}

fn is_global_uav(b: &D3D12_RESOURCE_BARRIER) -> bool {
    // SAFETY: `Type` discriminates `Anonymous.UAV`.
    b.Type == D3D12_RESOURCE_BARRIER_TYPE_UAV && unsafe { b.Anonymous.UAV.pResource.is_none() }
}

/// The resource a state change or aliasing names (its interface pointer).
fn named(b: &D3D12_RESOURCE_BARRIER) -> Option<usize> {
    // SAFETY: the union's member is the one `Type` says; a pointer read.
    unsafe {
        let p: Option<&ID3D12Resource> = match b.Type {
            D3D12_RESOURCE_BARRIER_TYPE_TRANSITION => b.Anonymous.Transition.pResource.as_ref(),
            D3D12_RESOURCE_BARRIER_TYPE_ALIASING => b.Anonymous.Aliasing.pResourceAfter.as_ref(),
            _ => None,
        };
        p.map(|r| windows::core::Interface::as_raw(r) as usize)
    }
}

impl std::ops::Deref for Rec {
    type Target = ID3D12GraphicsCommandList;

    /// The list, with what is held recorded first.
    fn deref(&self) -> &ID3D12GraphicsCommandList {
        if self.any.load(Relaxed) {
            self.flush();
        }
        &self.list
    }
}

/// For the frame-rate line.
pub(crate) fn line(frames: f64) {
    let [asked, made] = [0, 1].map(|i| STATS[i].swap(0, Relaxed) as f64 / frames);
    let [skipped, enhanced, legacy, coalesced] = [0, 1, 2, 3].map(|i| GROUP_STATS[i].swap(0, Relaxed) as f64 / frames);
    LAST_COALESCED_X10.store((coalesced * 10.0).round() as u64, Relaxed);
    if asked > 0.0 || skipped + enhanced + legacy > 0.0 {
        tracing::info!(
            "barrier calls per frame (VESPER_COALESCE_BARRIERS): {asked:.0} asked for, {made:.0} made of them; enhanced barrier groups (VESPER_SMART_BARRIERS): {skipped:.0} skipped, {enhanced:.0} global ({coalesced:.0} coalesced), {legacy:.0} legacy uav"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_global_preserves_common_and_ors_masks() {
        let a = D3D12_GLOBAL_BARRIER {
            SyncBefore: D3D12_BARRIER_SYNC_COMPUTE_SHADING,
            SyncAfter: D3D12_BARRIER_SYNC_PIXEL_SHADING,
            AccessBefore: D3D12_BARRIER_ACCESS_UNORDERED_ACCESS,
            AccessAfter: D3D12_BARRIER_ACCESS_SHADER_RESOURCE,
        };
        let b = D3D12_GLOBAL_BARRIER {
            SyncBefore: D3D12_BARRIER_SYNC_COPY,
            SyncAfter: D3D12_BARRIER_SYNC_COMPUTE_SHADING,
            AccessBefore: D3D12_BARRIER_ACCESS_COPY_DEST,
            AccessAfter: D3D12_BARRIER_ACCESS_UNORDERED_ACCESS,
        };
        let m = merge_global(a, b);
        assert_eq!(m.SyncBefore.0, D3D12_BARRIER_SYNC_COMPUTE_SHADING.0 | D3D12_BARRIER_SYNC_COPY.0);
        assert_eq!(m.SyncAfter.0, D3D12_BARRIER_SYNC_PIXEL_SHADING.0 | D3D12_BARRIER_SYNC_COMPUTE_SHADING.0);
        assert_eq!(
            m.AccessBefore.0,
            D3D12_BARRIER_ACCESS_UNORDERED_ACCESS.0 | D3D12_BARRIER_ACCESS_COPY_DEST.0
        );
        assert_eq!(
            m.AccessAfter.0,
            D3D12_BARRIER_ACCESS_SHADER_RESOURCE.0 | D3D12_BARRIER_ACCESS_UNORDERED_ACCESS.0
        );

        let with_common = D3D12_GLOBAL_BARRIER {
            AccessBefore: D3D12_BARRIER_ACCESS_COMMON,
            AccessAfter: D3D12_BARRIER_ACCESS_COMMON,
            ..b
        };
        let mc = merge_global(a, with_common);
        assert_eq!(mc.AccessBefore, D3D12_BARRIER_ACCESS_COMMON);
        assert_eq!(mc.AccessAfter, D3D12_BARRIER_ACCESS_COMMON);
        assert_eq!(mc.SyncBefore, D3D12_BARRIER_SYNC_ALL);
        assert_eq!(mc.SyncAfter, D3D12_BARRIER_SYNC_ALL);

        let sync_all_pair = D3D12_GLOBAL_BARRIER {
            SyncBefore: D3D12_BARRIER_SYNC_ALL,
            SyncAfter: D3D12_BARRIER_SYNC_ALL,
            ..b
        };
        let ma = merge_global(a, sync_all_pair);
        assert_eq!(ma.SyncBefore, D3D12_BARRIER_SYNC_ALL);
        assert_eq!(ma.SyncAfter, D3D12_BARRIER_SYNC_ALL);

        let with_common_before = D3D12_GLOBAL_BARRIER {
            SyncBefore: D3D12_BARRIER_SYNC_ALL,
            SyncAfter: D3D12_BARRIER_SYNC_PIXEL_SHADING,
            AccessBefore: D3D12_BARRIER_ACCESS_COMMON,
            AccessAfter: D3D12_BARRIER_ACCESS_SHADER_RESOURCE,
        };
        let mcb = merge_global(a, with_common_before);
        assert_eq!(mcb.AccessBefore, D3D12_BARRIER_ACCESS_COMMON);
        assert_eq!(mcb.AccessAfter, D3D12_BARRIER_ACCESS_COMMON);
        assert_eq!(mcb.SyncBefore, D3D12_BARRIER_SYNC_ALL);
        assert_eq!(mcb.SyncAfter, D3D12_BARRIER_SYNC_ALL);

        let rt_rt = D3D12_GLOBAL_BARRIER {
            SyncBefore: D3D12_BARRIER_SYNC_RENDER_TARGET,
            SyncAfter: D3D12_BARRIER_SYNC_RENDER_TARGET,
            AccessBefore: D3D12_BARRIER_ACCESS_RENDER_TARGET,
            AccessAfter: D3D12_BARRIER_ACCESS_RENDER_TARGET,
        };
        let mrt = merge_global(a, rt_rt);
        assert_eq!(mrt.AccessBefore, D3D12_BARRIER_ACCESS_COMMON);
        assert_eq!(mrt.AccessAfter, D3D12_BARRIER_ACCESS_COMMON);
        assert_eq!(mrt.SyncBefore, D3D12_BARRIER_SYNC_ALL);
        assert_eq!(mrt.SyncAfter, D3D12_BARRIER_SYNC_ALL);
    }
}
