use simba::simd::{SimdValue, WideBoolF32x8, WideF32x8};

pub trait SimbaWorkarounds: SimdValue {
    fn is_nan(self) -> Self::SimdBool;

    fn infinity() -> Self;
    fn neg_infinity() -> Self;
}

impl SimbaWorkarounds for WideF32x8 {
    #[inline(always)]
    fn is_nan(self) -> Self::SimdBool {
        WideBoolF32x8(self.0.is_nan())
    }

    #[inline(always)]
    fn infinity() -> Self {
        Self::splat(f32::INFINITY)
    }

    #[inline(always)]
    fn neg_infinity() -> Self {
        Self::splat(f32::NEG_INFINITY)
    }
}
