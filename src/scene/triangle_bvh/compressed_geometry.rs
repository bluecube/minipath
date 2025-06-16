//! This module contains compressed representations for points, boxes and unit vectors
//! to be used in the BVH scene representation.
//! These are all mapped into u16, saving 50% space

use assert2::debug_assert;

use simba::simd::{WideBoolF32x8, WideF32x8};
// This module uses wide directly, as simba doesn't support integer vectors
// This limits robustness to change -- if/when we move to std::simd
use wide::{CmpGe as _, CmpLe as _, f32x8, i32x8, u16x8};

use crate::geometry::{AABB, SimdFloatType, Triangle, WorldBox8, WorldPoint8, WorldVector8};

/// Represents 8 closed real intervals [0, 1] compressed to u16 each.
#[derive(Copy, Clone, Debug, Default)]
#[repr(transparent)]
struct UnitInterval8(u16x8);

impl UnitInterval8 {
    fn compress_internal(v: f32x8, rounding: &impl Fn(f32x8) -> f32x8) -> Self {
        debug_assert!(v.cmp_ge(f32x8::splat(-1e-6)).all(), "{v}");
        debug_assert!(v.cmp_le(f32x8::splat(1.0 + 1e-6)).all(), "{v}");
        let max = u16x8_to_f32x8(u16x8::MAX);
        Self(i32x8_to_u16x8(
            rounding(v * max).min(max).max(f32x8::ZERO).fast_trunc_int(),
        ))
    }

    pub fn decompress(&self) -> SimdFloatType {
        WideF32x8(u16x8_to_f32x8(self.0) / u16x8_to_f32x8(u16x8::MAX))
    }

    pub fn is_zero(&self) -> WideBoolF32x8 {
        WideBoolF32x8(u16x8_to_f32x8(self.0.cmp_eq(u16x8::ZERO)))
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct RelativePoint8 {
    x: UnitInterval8,
    y: UnitInterval8,
    z: UnitInterval8,
}

impl RelativePoint8 {
    pub fn compress(p: &WorldPoint8, enclosing_box: &WorldBox8) -> Self {
        Self::compress_internal(p, enclosing_box, &f32x8::round)
    }

    fn compress_internal(
        p: &WorldPoint8,
        enclosing_box: &WorldBox8,
        rounding: &impl Fn(f32x8) -> f32x8,
    ) -> Self {
        let relative = (p - enclosing_box.min).component_div(&enclosing_box.size());
        Self {
            x: UnitInterval8::compress_internal(relative.x.0, rounding),
            y: UnitInterval8::compress_internal(relative.y.0, rounding),
            z: UnitInterval8::compress_internal(relative.z.0, rounding),
        }
    }

    pub fn decompress(&self, enclosing_box: &WorldBox8) -> WorldPoint8 {
        let relative = WorldVector8::new(
            self.x.decompress(),
            self.y.decompress(),
            self.z.decompress(),
        );
        enclosing_box.min + relative.component_mul(&enclosing_box.size())
    }

    pub fn is_zero(&self) -> WideBoolF32x8 {
        self.x.is_zero() & self.y.is_zero() & self.z.is_zero()
    }
}

pub type RelativeBox8 = AABB<RelativePoint8>;

impl RelativeBox8 {
    pub fn compress_round_out(b: WorldBox8, enclosing_box: &WorldBox8) -> Self {
        RelativeBox8 {
            min: RelativePoint8::compress_internal(&b.min, enclosing_box, &f32x8::floor),
            max: RelativePoint8::compress_internal(&b.max, enclosing_box, &f32x8::ceil),
        }
    }

    pub fn decompress(&self, enclosing_box: &WorldBox8) -> WorldBox8 {
        self.map(|p| p.decompress(enclosing_box))
    }
}

pub type RelativeTriangle8 = Triangle<RelativePoint8>;

impl RelativeTriangle8 {
    pub fn compress(triangle: &Triangle<WorldPoint8>, enclosing_box: &WorldBox8) -> Self {
        dbg!(triangle);
        dbg!(enclosing_box);
        triangle.map(|p| RelativePoint8::compress(p, enclosing_box))
    }

    pub fn decompress(&self, enclosing_box: &WorldBox8) -> Triangle<WorldPoint8> {
        self.map(|p| p.decompress(enclosing_box))
    }
}

fn u16x8_to_f32x8(v: u16x8) -> f32x8 {
    f32x8::from_i32x8(i32x8::from_u16x8(v))
}

fn i32x8_to_u16x8(v: i32x8) -> u16x8 {
    u16x8::new([
        v.as_array_ref()[0] as _,
        v.as_array_ref()[1] as _,
        v.as_array_ref()[2] as _,
        v.as_array_ref()[3] as _,
        v.as_array_ref()[4] as _,
        v.as_array_ref()[5] as _,
        v.as_array_ref()[6] as _,
        v.as_array_ref()[7] as _,
    ])
}

#[cfg(test)]
mod test {
    use super::*;

    use assert2::assert;
    use simba::simd::SimdValue as _;
    use test_strategy::proptest;

    #[proptest]
    fn compressed_unit_interval_round_trip_round(#[strategy(0.0f32..=1.0f32)] v: f32) {
        let v_simd = SimdFloatType::splat(v);
        let i = UnitInterval8::compress_internal(v_simd.0, &f32x8::round);
        let decompressed = i.decompress();
        let max_error = 0.5 / (u16::MAX as f32);

        assert!(decompressed.extract(0) >= v - max_error);
        assert!(decompressed.extract(0) <= v + max_error);
    }
}
