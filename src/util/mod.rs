pub mod simba;

use std::array;

pub fn bit_iter(bits: u64) -> BitIter {
    BitIter { bits }
}

#[derive(Copy, Clone, Debug)]
pub struct BitIter {
    bits: u64,
}

impl Iterator for BitIter {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        if self.bits == 0 {
            return None;
        }
        let tz = self.bits.trailing_zeros() as usize;
        self.bits &= self.bits - 1;
        Some(tz)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let count = self.bits.count_ones() as usize;
        (count, Some(count))
    }
}

pub type Rgba = rgb::RGBA<f32>;

/// Converts an iterator into an array of N elements.
/// Ignores any extra values, fills missing values with defaults.
pub fn collect_to_array<T: Default, const N: usize>(values: impl IntoIterator<Item = T>) -> [T; N] {
    let mut iter = values.into_iter();
    array::from_fn(|_| iter.next().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::assert;

    #[test]
    fn bit_iter_basic() {
        let result: Vec<usize> = bit_iter(0b10101000u64).collect();
        assert!(result == vec![3, 5, 7]);
    }

    #[test]
    fn bit_iter_all_bits() {
        let result: Vec<usize> = bit_iter(u64::MAX).collect();
        assert!(result == (0..(u64::BITS as usize)).collect::<Vec<_>>());
    }

    #[test]
    fn bit_iter_empty() {
        let result: Vec<usize> = bit_iter(0).collect();
        assert!(result.is_empty());
    }

    #[test]
    fn collect_to_array_exact() {
        let input = [1, 2, 3];
        let output: [i32; 3] = collect_to_array(input);
        assert!(output == [1, 2, 3]);
    }

    #[test]
    fn collect_to_array_too_short() {
        let input = [1, 2];
        let output: [i32; 3] = collect_to_array(input);
        assert!(output == [1, 2, 0]);
    }

    #[test]
    fn collect_to_array_too_long() {
        let input = [1, 2, 3, 4, 5];
        let output: [i32; 3] = collect_to_array(input);
        assert!(output == [1, 2, 3]);
    }
}
