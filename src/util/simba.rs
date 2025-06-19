use simba::simd::{SimdBool, SimdValue, WideBoolF32x8, WideF32x8};

use crate::geometry::{SimdFloatType, WorldVector8};

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

/// Converts a flat iterator of elements into an iterator of SIMD values and mask.
/// If input iterator length is not divisible by T::LANES, remainder of the last
/// vector will be filled with the content of T::default() and mask will be false.
pub fn simd_windows<T: SimdValue + Default>(
    value: impl IntoIterator<Item = T::Element>,
) -> impl Iterator<Item = (T, T::SimdBool)>
where
    T::SimdBool: SimdValue,
    <T::SimdBool as SimdValue>::Element: From<bool>,
{
    let mut iter = value.into_iter();
    std::iter::from_fn(move || {
        let mut t = T::default();
        let mut mask = <T::SimdBool as SimdValue>::splat(false.into());

        for (j, v) in (0..T::LANES).zip(&mut iter) {
            t.replace(j, v);
            mask.replace(j, true.into());
        }

        if mask.any() { Some((t, mask)) } else { None }
    })
}

pub fn simd_element_iter<T: SimdValue>(value: T) -> impl Iterator<Item = T::Element> {
    (0..T::LANES).map(move |i| value.extract(i))
}

pub fn fma_dot(a: &WorldVector8, b: &WorldVector8) -> SimdFloatType {
    WideF32x8(a.z.0.mul_add(b.z.0, a.y.0.mul_add(b.y.0, a.x.0 * b.x.0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::assert;
    use proptest::{prelude::Strategy, prop_assert};
    use simba::simd::WideF32x8;
    use test_strategy::proptest;

    #[test]
    fn simd_windows_exact_fill() {
        let input = 0..16;
        let result: Vec<_> = simd_windows::<WideF32x8>(input.map(|x| x as f32)).collect();
        assert!(result.len() == 2);
        assert!(result[0].0.extract(0) == 0.0);
        assert!(result[0].0.extract(7) == 7.0);
        assert!(result[1].0.extract(0) == 8.0);
        assert!(result[1].0.extract(7) == 15.0);
    }

    #[test]
    fn simd_windows_partial_fill() {
        let input = 0..10;
        let result: Vec<_> = simd_windows::<WideF32x8>(input.map(|x| x as f32)).collect();
        assert!(result.len() == 2);
        assert!(result[0].0.extract(0) == 0.0);
        assert!(result[1].0.extract(1) == 9.0);
        assert!(!result[1].1.extract(2));
        assert!(!result[1].1.extract(7));
    }

    #[test]
    fn simd_windows_empty() {
        let input = std::iter::empty::<f32>();
        let result: Vec<_> = simd_windows::<WideF32x8>(input).collect();
        assert!(result.is_empty());
    }

    fn simd_value_strategy() -> impl Strategy<Value = SimdFloatType> {
        proptest::array::uniform8(-1e3f32..1e3f32).prop_map_into()
    }

    fn world_vector8_strategy() -> impl Strategy<Value = WorldVector8> {
        (
            simd_value_strategy(),
            simd_value_strategy(),
            simd_value_strategy(),
        )
            .prop_map(|(x, y, z)| WorldVector8::new(x, y, z))
    }

    #[proptest]
    fn fma_dot_matches_nalgebra_dot(
        #[strategy(world_vector8_strategy())] a: WorldVector8,
        #[strategy(world_vector8_strategy())] b: WorldVector8,
    ) {
        let expected = a.dot(&b);
        let actual = fma_dot(&a, &b);

        // Allow slight float inaccuracy
        for i in 0..8 {
            let e = expected.extract(i);
            let a = actual.extract(i);

            let difference = (e - a).abs();
            prop_assert!(
                difference < 1e-3 || difference < e.abs() * 1e-3,
                "Mismatch at lane {}: expected {}, got {}",
                i,
                e,
                a
            );
        }
    }
}
