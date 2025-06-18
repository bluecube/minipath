pub mod simba;
mod stats;

pub use stats::Stats;

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
}
