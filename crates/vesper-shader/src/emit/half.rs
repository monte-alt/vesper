//! 16-bit float arithmetic in native `half` (module notes, "16-bit
//! floats"): on hosts with 16-bit operations, the f16 operations whose
//! `half` result is the console's are made in `half` instead of f32.

use super::{Lower, Why, flags, host_rdna};
use crate::emit::bitcode::{Attr, Bin, Cast, V, pred};
use crate::gfx10::{Fields, Operand};
use crate::ir::{Inst, Reg};

impl Lower<'_> {
    /// Whether 16-bit float arithmetic is done in native `half` here
    /// (module notes, "16-bit floats"): the host has it
    /// ([`super::Parts::native16`]). `half` arithmetic rounds to nearest
    /// even, the console's MODE default (the MODE register is not modelled
    /// here: the f32 path converts with the host's own rounding).
    pub(super) fn native16(&self) -> bool {
        self.x.native16
    }

    /// The low 16 bits of `x` as a `half`.
    pub(super) fn as_half(&mut self, x: V) -> V {
        // Native 16-bit types: the module says so (cleared again if the
        // clean-up removes every use).
        self.s.flags |= flags::LOW_PRECISION | flags::NATIVE_LOW_PRECISION;
        let c = self.cur;
        let i16t = self.s.m.int(16);
        let t = self.s.m.cast(c, Cast::Trunc, x, i16t);
        let ht = self.s.m.half();
        self.s.m.cast(c, Cast::Bitcast, t, ht)
    }

    /// The bits of the `half` `h` in the low half of an i32, the high half
    /// zero.
    pub(super) fn half_bits(&mut self, h: V) -> V {
        let c = self.cur;
        let i16t = self.s.m.int(16);
        let t = self.s.m.cast(c, Cast::Bitcast, h, i16t);
        self.s.m.cast(c, Cast::ZExt, t, self.s.t.i32)
    }

    /// A `dx.op` on `half` operands given as bits (the low 16 of each),
    /// its result as bits.
    pub(super) fn half_op(&mut self, name: &str, op: u32, xs: &[V]) -> V {
        let hs: Vec<V> = xs.iter().map(|&x| self.as_half(x)).collect();
        let ht = self.s.m.half();
        let r = self.s.op(self.cur, name, Attr::ReadNone, ht, op, &hs);
        self.half_bits(r)
    }

    /// `a op b` in `half`, of f16 bits, as bits.
    pub(super) fn half_bin(&mut self, op: Bin, a: V, b: V) -> V {
        let (a, b) = (self.as_half(a), self.as_half(b));
        let r = self.s.m.bin(self.cur, op, a, b);
        self.half_bits(r)
    }

    /// `a * b + c` of f16 bits in `half`, rounded once where the host fuses
    /// its 16-bit `FMad` (module notes, "16-bit floats").
    pub(super) fn fma16(&mut self, a: V, b: V, c: V) -> V {
        self.half_op("tertiary.f16", 46, &[a, b, c])
    }

    /// [`Self::max_min`] of f16 bits (the high halves zero) in `half`: of two
    /// zeros +0 unless both are -0 (max), -0 unless neither (min); the
    /// console keeps 16-bit denormals, so nothing is flushed. On RDNA
    /// ([`super::super::rdna::MAX_MIN`]) the host's own maximum, whose
    /// instruction orders the zeros so.
    pub(super) fn max_min16(&mut self, max: bool, a: V, b: V) -> V {
        let c = self.cur;
        let r = self.half_op("binary.f16", if max { 35 } else { 36 }, &[a, b]);
        if host_rdna() & super::super::rdna::MAX_MIN != 0 {
            return r;
        }
        let either = self.s.m.bin(c, Bin::Or, a, b);
        let magnitude = self.s.m.i32(0x7FFF);
        let either = self.s.m.bin(c, Bin::And, either, magnitude);
        let zero = self.s.m.i32(0);
        let zeros = self.s.m.cmp(c, pred::EQ, either, zero);
        let op = if max { Bin::And } else { Bin::Or };
        let sign = self.s.m.bin(c, op, a, b);
        self.s.m.select(c, zeros, sign, r)
    }

    /// Whether the f16 bits `x` (the high half zero) are a NaN.
    pub(super) fn nan16(&mut self, x: V) -> V {
        let c = self.cur;
        let magnitude = self.s.m.i32(0x7FFF);
        let a = self.s.m.bin(c, Bin::And, x, magnitude);
        let inf = self.s.m.i32(0x7C00);
        self.s.m.cmp(c, pred::IUGT, a, inf)
    }

    /// [`Self::med3`] of f16 bits in `half`: with a NaN among them, the
    /// minimum of the three.
    pub(super) fn med3_16(&mut self, a: V, b: V, third: V) -> V {
        let c = self.cur;
        let lo = self.max_min16(false, a, b);
        let hi = self.max_min16(true, a, b);
        let t = self.max_min16(false, hi, third);
        let median = self.max_min16(true, lo, t);
        let least = self.max_min16(false, lo, third);
        let (na, nb, nc) = (self.nan16(a), self.nan16(b), self.nan16(third));
        let nan = self.s.m.bin(c, Bin::Or, na, nb);
        let nan = self.s.m.bin(c, Bin::Or, nan, nc);
        self.s.m.select(c, nan, least, median)
    }

    /// The clamp of an f16 result (bits, the high half zero) to [0, 1]: +0
    /// for NaN and for -0, decided on the bits, where the host's saturate
    /// may keep a -0.
    pub(super) fn clamp16(&mut self, x: V) -> V {
        let c = self.cur;
        // Unsigned, every negative half (-0 and negative NaNs included)
        // and every positive NaN lies above +infinity.
        let inf = self.s.m.i32(0x7C00);
        let bad = self.s.m.cmp(c, pred::IUGT, x, inf);
        let one = self.s.m.i32(0x3C00);
        let big = self.s.m.cmp(c, pred::IUGE, x, one);
        let r = self.s.m.select(c, big, one, x);
        let zero = self.s.m.i32(0);
        self.s.m.select(c, bad, zero, r)
    }

    /// An f16 source's bits (the high half zero) with its abs/neg
    /// modifiers, which only touch the sign.
    pub(super) fn half_bits_src(&mut self, inst: &Inst, k: usize) -> Result<V, Why> {
        let Fields::Valu(v) = &inst.fields else {
            unreachable!()
        };
        let mut x = self.half_src(inst, k)?;
        let c = self.cur;
        if v.abs >> k & 1 != 0 {
            let m = self.s.m.i32(0x7FFF);
            x = self.s.m.bin(c, Bin::And, x, m);
        }
        if v.neg >> k & 1 != 0 {
            let m = self.s.m.i32(0x8000);
            x = self.s.m.bin(c, Bin::Xor, x, m);
        }
        Ok(x)
    }

    /// The accumulator of `v_fmac_f16`/`v_mac_f16`: the destination's half
    /// that is written, as bits (the high half zero).
    pub(super) fn mac_acc_bits(&mut self, inst: &Inst) -> Result<V, Why> {
        let Fields::Valu(v) = &inst.fields else {
            unreachable!()
        };
        let c = self.cur;
        let Some(Operand::Vector { reg, .. }) = v.dst else {
            return Err("an accumulator");
        };
        let x = self.i32_of(inst.read(Reg::V(reg)).ok_or("an accumulator")?)?;
        let x = if v.opsel >> 3 & 1 != 0 {
            let k16 = self.s.m.i32(16);
            self.s.m.bin(c, Bin::LShr, x, k16)
        } else {
            x
        };
        let m = self.s.m.i32(0xFFFF);
        Ok(self.s.m.bin(c, Bin::And, x, m))
    }

    /// [`Self::valu16`] in native `half` ([`Self::native16`]), for the
    /// operations whose `half` result is the console's: sums, differences
    /// and products (rounded once to nearest even), fused multiply-adds
    /// (rounded once where the host fuses), minima, maxima and medians
    /// (exact). `None` for the others, and for an instruction with an
    /// output modifier: that takes the f32 path, whatever it does with the
    /// modifier (both paths then agree on what translates).
    pub(super) fn valu16_native(
        &mut self,
        inst: &Inst,
        name: &str,
        guard: Option<V>,
    ) -> Option<Result<(), Why>> {
        const NATIVE: &[&str] = &[
            "v_add_f16",
            "v_sub_f16",
            "v_subrev_f16",
            "v_mul_f16",
            "v_max_f16",
            "v_min_f16",
            "v_fma_f16",
            "v_fmac_f16",
            "v_max3_f16",
            "v_min3_f16",
            "v_med3_f16",
        ];
        let Fields::Valu(v) = &inst.fields else {
            return None;
        };
        if !self.native16() || !NATIVE.contains(&name) || v.omod != 0 {
            return None;
        }
        Some(self.valu16_native_op(inst, name, guard))
    }

    fn valu16_native_op(&mut self, inst: &Inst, name: &str, guard: Option<V>) -> Result<(), Why> {
        let Fields::Valu(v) = &inst.fields else {
            unreachable!()
        };
        let (a, b) = (self.half_bits_src(inst, 0)?, self.half_bits_src(inst, 1)?);
        let r = match name {
            "v_add_f16" => self.half_bin(Bin::Add, a, b),
            "v_sub_f16" => self.half_bin(Bin::Sub, a, b),
            "v_subrev_f16" => self.half_bin(Bin::Sub, b, a),
            "v_mul_f16" => self.half_bin(Bin::Mul, a, b),
            "v_max_f16" => self.max_min16(true, a, b),
            "v_min_f16" => self.max_min16(false, a, b),
            _ => {
                let third = if name == "v_fmac_f16" {
                    self.mac_acc_bits(inst)?
                } else {
                    self.half_bits_src(inst, 2)?
                };
                match name {
                    "v_fma_f16" | "v_fmac_f16" => self.fma16(a, b, third),
                    "v_max3_f16" => {
                        let t = self.max_min16(true, a, b);
                        self.max_min16(true, t, third)
                    }
                    "v_min3_f16" => {
                        let t = self.max_min16(false, a, b);
                        self.max_min16(false, t, third)
                    }
                    _ => self.med3_16(a, b, third),
                }
            }
        };
        let r = if v.clamp { self.clamp16(r) } else { r };
        self.write_half(inst, r, guard)
    }
}
