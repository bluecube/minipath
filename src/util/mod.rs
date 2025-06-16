pub mod simba;

use std::{array, fmt::Display};

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

#[derive(Clone, Debug)]
pub struct Stats {
    pub count: usize,
    pub min: usize,
    pub max: usize,
    pub avg: f32,
}

impl Stats {
    pub fn new_single(v: usize) -> Self {
        Stats {
            count: 1,
            min: v,
            max: v,
            avg: v as f32,
        }
    }

    pub fn add_sample(&mut self, value: usize) {
        self.count += 1;
        self.min = self.min.min(value);
        self.max = self.max.max(value);
        self.avg += (value as f32 - self.avg) / (self.count as f32);
    }

    pub fn add_samples(&mut self, it: impl IntoIterator<Item = usize>) {
        for v in it {
            self.add_sample(v);
        }
    }

    pub fn merge(&self, other: &Self) -> Self {
        Stats {
            count: self.count + other.count,
            min: self.min.min(other.min),
            max: self.max.max(other.max),
            avg: (self.avg * self.count as f32 + other.avg * other.count as f32)
                / (self.count + other.count) as f32,
        }
    }
}

impl Default for Stats {
    fn default() -> Self {
        Stats {
            count: 0,
            min: usize::MAX,
            max: 0,
            avg: 0.0,
        }
    }
}

impl Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} - {}; avg {:.1}; {} samples",
            self.min, self.max, self.avg, self.count
        )
    }
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
