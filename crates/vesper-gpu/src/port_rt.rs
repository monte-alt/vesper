//! Renderer port RT: ray tracing's own costs (`VESPER_PORT_RT=1`, off by
//! default; read in [`on`]).
//!
//! Part 1, the lane's wait for the card. With the structure builder on, one
//! submission a frame stood until every flight had landed because of a
//! constant buffer in upload memory read on the CPU at submission (`Step::Fill`)
//! over bytes a copy at its place had declared written, refused for
//! `BEHIND_PLACED`. With the switch, that copy only stands in the way when it
//! is ahead of a card step, submissions are cut behind it when in a later list,
//! and a fill behind a copy at its place in the same list forwards its source
//! (`HostList::forward_fill`, [`forwarded`]).

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// Whether the port is on. A/B-able as `VESPER_AB=<s>,VESPER_PORT_RT` in
/// diagnostics builds; on in tests for the threads that ask for it.
pub(crate) fn on() -> bool {
    #[cfg(feature = "diagnostics")]
    if let Some(on) = crate::ab::pick("VESPER_PORT_RT") {
        return on;
    }
    #[cfg(test)]
    if PORT_RT_IN_TEST.get() {
        return true;
    }
    // Read here: the switch of renderer port RT.
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("VESPER_PORT_RT").is_ok_and(|v| v.trim() == "1"))
}

#[cfg(test)]
thread_local! {
    /// [`on`], for what this thread does.
    pub(crate) static PORT_RT_IN_TEST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Since the last line: submissions whose copies at their place stood in the
/// card's way only by the old rule (the order lets them through); cut behind
/// such a copy; left to wait, as the step in its way is in the copy's own list;
/// submissions refused for it with the switch off.
static COUNTS: [AtomicU64; 5] = [const { AtomicU64::new(0) }; 5];
pub(crate) const BY_ORDER: usize = 0;
pub(crate) const CUT: usize = 1;
pub(crate) const SAME_LIST: usize = 2;
pub(crate) const REFUSED_OFF: usize = 3;
/// Fills that take their bytes out of the source of a copy at its place ahead
/// of them in their list (`HostList::forward_fill`).
pub(crate) const FORWARDED: usize = 4;

pub(crate) fn count(what: usize) {
    COUNTS[what].fetch_add(1, Relaxed);
}

/// What a step of a list does, as [`forwarded`] sees it.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Seen {
    /// A copy at its place: destination, source, bytes, and whether the card
    /// copies what the CPU takes out of the source at submission.
    Placed { dst: usize, src: usize, len: usize, from_upload: bool },
    /// It writes title memory `[from, to)`, or declares a shader write there.
    Writes(usize, usize),
    /// It may write anywhere.
    Anything,
    /// It writes no title memory.
    Nothing,
}

/// Where a fill of `[src, src + len)` recorded now takes its bytes, given the
/// list's steps so far, last first; `None`: out of `src` itself. See the
/// module: the last step that writes any of them is a copy at its place that
/// holds all of them, copies out of upload memory, is not onto itself, and
/// nothing after it writes its source.
pub(crate) fn forwarded(steps: impl Iterator<Item = Seen>, src: usize, len: usize) -> Option<usize> {
    let to = src.checked_add(len)?;
    if len == 0 {
        return None;
    }
    let meet = |a: usize, b: usize, c: usize, d: usize| a < d && c < b;
    // What the steps after it write.
    let mut later: Vec<(usize, usize)> = Vec::new();
    for step in steps {
        let (from, until) = match step {
            Seen::Placed { dst, src: from, len: n, from_upload } => {
                if meet(dst, dst + n, src, to) {
                    let holds = dst <= src && to <= dst + n;
                    let moved = later.iter().any(|&(a, b)| meet(a, b, from, from + n));
                    let apart = !meet(from, from + n, dst, dst + n);
                    return (holds && !moved && apart && from_upload).then(|| from + (src - dst));
                }
                (dst, dst + n)
            }
            Seen::Writes(a, b) => (a, b),
            Seen::Anything => return None,
            Seen::Nothing => continue,
        };
        if meet(from, until, src, to) {
            return None;
        }
        later.push((from, until));
    }
    None
}

/// Index (in forward list order) of the `Placed` copy that last wrote all of
/// `[src, src + len)` in this list without being superseded by a later CPU
/// write or `Run`, when [`forwarded`] cannot forward the read on the CPU.
pub(crate) fn placed_gpu_source(
    steps: impl Iterator<Item = (usize, Seen)> + Clone,
    src: usize,
    len: usize,
) -> Option<usize> {
    let to = src.checked_add(len)?;
    if len == 0 {
        return None;
    }
    if forwarded(steps.clone().map(|(_, s)| s), src, len).is_some() {
        return None;
    }
    let meet = |a: usize, b: usize, c: usize, d: usize| a < d && c < b;
    for (idx, step) in steps {
        match step {
            Seen::Placed { dst, len: n, .. } if meet(dst, dst + n, src, to) => {
                return (dst <= src && to <= dst + n).then_some(idx);
            }
            Seen::Writes(a, b) if meet(a, b, src, to) => return None,
            Seen::Anything => return None,
            _ => {}
        }
    }
    None
}

/// The port's line of the frame lines, per frame: printed with the switch on
/// or with something counted.
pub(crate) fn line(frames: f64) {
    let n: Vec<f64> = COUNTS.iter().map(|c| c.swap(0, Relaxed) as f64 / frames).collect();
    if on() || n.iter().any(|&v| v > 0.0) {
        tracing::info!(
            "renderer port RT (VESPER_PORT_RT={}) per frame: submissions with a copy at its place over what the card brings: \
             {:.2} let through by the order, {:.2} cut behind the copy, {:.2} left to wait (the step in the copy's own list); \
             {:.2} refused with the switch off; {:.1} fills that take a copy's bytes out of its source",
            u8::from(on()),
            n[BY_ORDER],
            n[CUT],
            n[SAME_LIST],
            n[REFUSED_OFF],
            n[FORWARDED],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{Seen, forwarded};

    fn placed(dst: usize, src: usize, len: usize) -> Seen {
        Seen::Placed { dst, src, len, from_upload: true }
    }

    /// Last first, as a list's steps are walked.
    fn walk(steps: &[Seen], src: usize, len: usize) -> Option<usize> {
        forwarded(steps.iter().rev().copied(), src, len)
    }

    fn walk_gpu(steps: &[Seen], src: usize, len: usize) -> Option<usize> {
        super::placed_gpu_source(steps.iter().enumerate().rev().map(|(i, &s)| (i, s)), src, len)
    }

    #[test]
    fn gpu_source_avoids_behind_placed_when_cpu_forward_cannot() {
        // GPU->GPU placed copy (from_upload: false) followed by a constant/scratch read
        // in the same list: cannot forward on CPU, so returns placed step index 0.
        let gpu_placed = Seen::Placed { dst: 0x1000, src: 0x8000, len: 0xbd0, from_upload: false };
        assert_eq!(walk(&[gpu_placed], 0x1000, 0xf0), None);
        assert_eq!(walk_gpu(&[gpu_placed], 0x1000, 0xf0), Some(0));
        // CPU-forwardable placed copy still uses fast CPU forwarding (returns None here).
        let cpu_placed = placed(0x1000, 0x8000, 0xbd0);
        assert_eq!(walk_gpu(&[cpu_placed], 0x1000, 0xf0), None);
        // Upload placed copy whose source was later overwritten falls back to GPU source.
        assert_eq!(walk_gpu(&[cpu_placed, Seen::Writes(0x8000, 0x8040)], 0x1000, 0xf0), Some(0));
        // Overwritten destination or Run invalidates the placed source.
        assert_eq!(walk_gpu(&[gpu_placed, Seen::Writes(0x1000, 0x1020)], 0x1000, 0xf0), None);
        assert_eq!(walk_gpu(&[gpu_placed, Seen::Anything], 0x1000, 0xf0), None);
    }

    #[test]
    fn a_fill_behind_a_copy_that_holds_it_reads_the_copys_source() {
        // The case of the runs: 0xb0 bytes copied, then read as constants.
        assert_eq!(walk(&[placed(0x1000, 0x8000, 0xb0)], 0x1000, 0xb0), Some(0x8000));
        // Part of it, at its offset.
        assert_eq!(walk(&[placed(0x1000, 0x8000, 0x100)], 0x1040, 0x20), Some(0x8040));
        // Steps that touch neither range between them change nothing.
        let between = [placed(0x1000, 0x8000, 0xb0), Seen::Nothing, Seen::Writes(0x9000, 0x9100), placed(0x3000, 0x4000, 16)];
        assert_eq!(walk(&between, 0x1000, 0xb0), Some(0x8000));
    }

    #[test]
    fn anything_else_reads_where_it_did() {
        // Not all of it.
        assert_eq!(walk(&[placed(0x1000, 0x8000, 0xb0)], 0x1080, 0x40), None);
        // Nothing ahead, or a copy elsewhere.
        assert_eq!(walk(&[], 0x1000, 0xb0), None);
        assert_eq!(walk(&[placed(0x2000, 0x8000, 0xb0)], 0x1000, 0xb0), None);
        // The card copies a host copy shaders write, not what the CPU took.
        assert_eq!(walk(&[Seen::Placed { dst: 0x1000, src: 0x8000, len: 0xb0, from_upload: false }], 0x1000, 0xb0), None);
        // Onto itself.
        assert_eq!(walk(&[placed(0x1000, 0x1040, 0x100)], 0x1000, 0x40), None);
        // A later write of the destination (a CPU copy, a shader), of the
        // source, or a step of ours.
        for after in [Seen::Writes(0x10a0, 0x10b0), Seen::Writes(0x8010, 0x8020), Seen::Anything] {
            assert_eq!(walk(&[placed(0x1000, 0x8000, 0xb0), after], 0x1000, 0xb0), None, "{after:?}");
        }
        // The last writer is another copy that holds only part of it.
        assert_eq!(walk(&[placed(0x1000, 0x8000, 0xb0), placed(0x1000, 0x9000, 0x10)], 0x1000, 0xb0), None);
        // The last writer is the copy that holds it: the one before is behind.
        assert_eq!(walk(&[placed(0x1000, 0x9000, 0x10), placed(0x1000, 0x8000, 0xb0)], 0x1000, 0xb0), Some(0x8000));
        // Empty, or past the end of the address space.
        assert_eq!(walk(&[placed(0x1000, 0x8000, 0xb0)], 0x1000, 0), None);
        assert_eq!(walk(&[placed(0x1000, 0x8000, 0xb0)], usize::MAX, 2), None);
    }

    #[test]
    fn the_switch_is_off_unless_asked_for() {
        assert!(!super::PORT_RT_IN_TEST.get());
        super::PORT_RT_IN_TEST.set(true);
        assert!(super::on());
        super::PORT_RT_IN_TEST.set(false);
    }
}
