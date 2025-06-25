//! This module contains compressed representations for points, boxes and unit vectors
//! to be used in the BVH scene representation.
//! These are all mapped into u16, saving 50% space

use assert2::debug_assert;

use simba::simd::{SimdBool as _, SimdPartialOrd as _, WideBoolF32x8, WideF32x8};
// This module uses wide directly, as simba doesn't support integer vectors
// This limits robustness to change -- if/when we move to std::simd
use wide::{CmpGe as _, CmpLe as _, f32x8, i32x8, u16x8};

use crate::geometry::{
    AABB, SimdFloatType, SimdMaskType, Triangle, WorldBox8, WorldBoxSized8, WorldPoint8,
    WorldVector8,
};

/// Represents 8 closed real intervals [0, 1] compressed to u16 each.
#[derive(Copy, Clone, Debug, Default)]
#[repr(transparent)]
struct UnitInterval8(u16x8);

impl UnitInterval8 {
    /// Compresses the interval.
    /// Positions where mask is false are turned to zero.
    fn compress_internal(
        v: f32x8,
        rounding: &impl Fn(f32x8) -> f32x8,
        mask: &SimdMaskType,
    ) -> Self {
        debug_assert!(
            (v.cmp_ge(f32x8::splat(-1e-6)) | !mask.0).all(),
            "v: {v}, mask: {mask:?}"
        );
        debug_assert!(
            (v.cmp_le(f32x8::splat(1.0 + 1e-6)) | !mask.0).all(),
            "v: {v}, mask: {mask:?}"
        );
        let max = u16x8_to_f32x8(u16x8::MAX);
        Self(i32x8_to_u16x8(
            mask.0
                .blend(rounding(v * max), f32x8::ZERO)
                .fast_min(max)
                .fast_max(f32x8::ZERO)
                .fast_trunc_int(),
        ))
    }

    pub fn decompress(&self) -> SimdFloatType {
        const INV_U16_MAX: f32 = 1.0 / (u16::MAX as f32);
        WideF32x8(u16x8_to_f32x8(self.0) * f32x8::splat(INV_U16_MAX))
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
    /// Compresses the point relative to enclosing box rounding to nearest.
    /// Positions where mask is true are turned to zero (= minimum of the enclosing box).
    pub fn compress(p: &WorldPoint8, enclosing_box: &WorldBoxSized8, mask: &SimdMaskType) -> Self {
        Self::compress_internal(p, enclosing_box, &f32x8::round, mask)
    }

    /// Compresses the point relative to enclosing box with given rounding.
    /// Positions where mask is false are turned to zero (= minimum of the enclosing box).
    fn compress_internal(
        p: &WorldPoint8,
        enclosing_box: &WorldBoxSized8,
        rounding: &impl Fn(f32x8) -> f32x8,
        mask: &SimdMaskType,
    ) -> Self {
        debug_assert!(
            enclosing_box
                .size
                .fold(true, |acc, x| acc & x.simd_gt(WideF32x8::ZERO).all()),
            "{:?}",
            enclosing_box.size
        );
        let relative = (p - enclosing_box.min).component_div(&enclosing_box.size);
        Self {
            x: UnitInterval8::compress_internal(relative.x.0, rounding, mask),
            y: UnitInterval8::compress_internal(relative.y.0, rounding, mask),
            z: UnitInterval8::compress_internal(relative.z.0, rounding, mask),
        }
    }

    pub fn decompress(&self, enclosing_box: &WorldBoxSized8) -> WorldPoint8 {
        let relative = WorldVector8::new(
            self.x.decompress(),
            self.y.decompress(),
            self.z.decompress(),
        );
        // The following block is a FMA equivalent of this:
        // enclosing_box.min + relative.component_mul(&enclosing_box.size())
        WorldPoint8 {
            coords: enclosing_box.size.zip_zip_map(
                &relative,
                &enclosing_box.min.coords,
                |size, relative, min| WideF32x8(size.0.mul_add(relative.0, min.0)),
            ),
        }
    }

    pub fn is_zero(&self) -> WideBoolF32x8 {
        self.x.is_zero() & self.y.is_zero() & self.z.is_zero()
    }
}

pub type RelativeBox8 = AABB<RelativePoint8>;

impl RelativeBox8 {
    /// Compresses the box relative to enclosing box, rounding to nearest.
    /// Positions where mask is false are turned to zero (= minimum of the enclosing box).
    pub fn compress_round_out(
        b: WorldBox8,
        enclosing_box: &WorldBoxSized8,
        mask: &SimdMaskType,
    ) -> Self {
        RelativeBox8 {
            min: RelativePoint8::compress_internal(&b.min, enclosing_box, &f32x8::floor, mask),
            max: RelativePoint8::compress_internal(&b.max, enclosing_box, &f32x8::ceil, mask),
        }
    }

    pub fn decompress(&self, enclosing_box: &WorldBoxSized8) -> WorldBox8 {
        self.map(|p| p.decompress(enclosing_box))
    }
}

impl Default for RelativeBox8 {
    fn default() -> Self {
        AABB {
            min: Default::default(),
            max: Default::default(),
        }
    }
}

pub type RelativeTriangle8 = Triangle<RelativePoint8>;

impl RelativeTriangle8 {
    /// Compresses the triangle relative to enclosing box, rounding to nearest.
    /// Positions where mask is false are turned to zero (= minimum of the enclosing box).
    pub fn compress(
        triangle: &Triangle<WorldPoint8>,
        enclosing_box: &WorldBoxSized8,
        mask: &SimdMaskType,
    ) -> Self {
        triangle.map(|p| RelativePoint8::compress(p, enclosing_box, mask))
    }

    pub fn decompress(&self, enclosing_box: &WorldBoxSized8) -> Triangle<WorldPoint8> {
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
        let i =
            UnitInterval8::compress_internal(v_simd.0, &f32x8::round, &SimdMaskType::splat(true));
        let decompressed = i.decompress();
        let max_error = 0.5 / (u16::MAX as f32);

        assert!(decompressed.extract(0) >= v - max_error);
        assert!(decompressed.extract(0) <= v + max_error);
    }
}
