use nalgebra::Point2;
use ordered_float::OrderedFloat;
use rand_distr::Distribution as _;
use std::iter::FusedIterator;
use std::num::NonZeroU32;

use crate::geometry::*;

impl ScreenBlock {
    pub fn is_empty(&self) -> bool {
        !(self.min < self.max)
    }

    pub fn area(&self) -> u32 {
        if self.is_empty() {
            0
        } else {
            self.size().product()
        }
    }

    pub fn contains(&self, p: &ScreenPoint) -> bool {
        (p >= &self.min) && (p < &self.max)
    }

    /// Create an iterator over coordinates (x, y) pairs inside the block,
    /// in C order (x changes first, then y)
    pub fn internal_points(&self) -> InternalPoints {
        if self.is_empty() {
            InternalPoints::empty()
        } else {
            InternalPoints {
                min_x: self.min.x,
                max: self.max,

                cursor: self.min,
            }
        }
    }

    /// Create a vec sub blocks in a randomized order, starting in the middle of the block.
    /// Tiles are tile_size * tile_size large, except on the bottom and right side of the
    /// block, where they may be clipped if tile size doesn't evenly divide block size.
    /// May panic if tile size is small (1 or 2) and block size is very large.
    /// This could be much simpler, but I like how the pattern looks when rendering :)
    pub fn tile_ordering(&self, tile_size: NonZeroU32) -> Vec<ScreenBlock> {
        if self.is_empty() {
            return Vec::new();
        }

        let center = self.center().cast::<f32>();

        let [min_x, min_y] = self.min.coords.into();
        let [max_x, max_y] = self.max.coords.into();

        let x_iter = divide_range(min_x, max_x, tile_size); // We construct x_iter only for size_hint...
        let y_iter = divide_range(min_y, max_y, tile_size);

        let mut tiles = Vec::with_capacity(x_iter.size_hint().0 * y_iter.size_hint().0);

        let randomness_scale = center.coords.norm() * 0.1;
        let distribution = rand_distr::Exp::new(1.0 / randomness_scale).unwrap();

        for (tile_min_y, tile_max_y) in y_iter {
            for (tile_min_x, tile_max_x) in divide_range(min_x, max_x, tile_size) {
                let tile = ScreenBlock::new(
                    ScreenPoint::new(tile_min_x, tile_min_y),
                    ScreenPoint::new(tile_max_x, tile_max_y),
                );

                let to_center = center - tile.center().cast::<f32>();

                tiles.push((
                    tile,
                    OrderedFloat(to_center.norm() + distribution.sample(&mut rand::rng())),
                ));
            }
        }

        tiles.sort_unstable_by_key(|(_tile, key)| key.clone());
        tiles.into_iter().map(|(tile, _key)| tile).collect()
    }
}

#[derive(Copy, Clone, Debug)]
pub struct InternalPoints {
    min_x: u32,
    max: ScreenPoint,

    cursor: ScreenPoint,
}

impl InternalPoints {
    // Construct an iterator over internal points that returns no points
    fn empty() -> Self {
        InternalPoints {
            min_x: 1,
            max: Point2::origin(),

            cursor: Point2::origin(),
        }
    }
}

impl Iterator for InternalPoints {
    type Item = ScreenPoint;

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor.y >= self.max.y {
            return None;
        }

        let ret = self.cursor;

        debug_assert!(self.cursor.x < self.max.x);
        self.cursor.x += 1;
        if self.cursor.x >= self.max.x {
            self.cursor.x = self.min_x;
            self.cursor.y += 1;
        }

        Some(ret)
    }
}

impl ExactSizeIterator for InternalPoints {
    fn len(&self) -> usize {
        if self.cursor.y >= self.max.y {
            0
        } else {
            let whole_rows = (self.max.y - self.cursor.y - 1) * (self.max.x - self.min_x);
            let current_row = self.max.x - self.cursor.x;
            (whole_rows + current_row) as usize
        }
    }
}

impl FusedIterator for InternalPoints {}

fn divide_range(start: u32, end: u32, tile_size: NonZeroU32) -> impl Iterator<Item = (u32, u32)> {
    let tile_size = tile_size.get();
    let total = end - start;
    let full_tiles = total / tile_size;
    let n = full_tiles
        + if full_tiles * tile_size != total {
            1
        } else {
            0
        };

    (0..n).map(move |i| {
        let tile_start = start + i * tile_size;
        let tile_end = end.min(tile_start + tile_size);
        (tile_start, tile_end)
    })
}

#[cfg(test)]
mod test {
    use super::*;
    use assert2::assert;
    use proptest::prelude::Strategy;
    use test_strategy::proptest;

    fn screen_block_strategy() -> impl proptest::strategy::Strategy<Value = ScreenBlock> {
        (0u32..1000u32, 0u32..1000u32, 0u32..1000u32, 0u32..1000u32)
            .prop_map(|(x, y, w, h)| ScreenBlock::new([x, y].into(), [x + w, y + h].into()))
    }

    fn check_exact_length_internal<T: Iterator + ExactSizeIterator>(
        iterator: &T,
        expected_length: usize,
    ) {
        assert!(iterator.len() == expected_length);
        let (min, max) = iterator.size_hint();
        assert!(min == expected_length);
        assert!(max.unwrap() == expected_length);
    }

    /// Goes through the whole iterator and checks that at every step iterator's size hint is equal
    /// to its reported length and equal to the expected number of elements.
    fn check_exact_length<T: Iterator + ExactSizeIterator>(
        mut iterator: T,
        expected_length: usize,
    ) {
        check_exact_length_internal(&iterator, expected_length);

        let mut count = 0usize;
        while let Some(_) = iterator.next() {
            count += 1;
            check_exact_length_internal(&iterator, expected_length - count);
        }
    }

    /// Check that all pixels in the block are covered by a pixel iterator
    fn check_pixel_iterator_covers_block<T: Iterator<Item = ScreenPoint>>(
        mut pixel_iterator: T,
        block: ScreenBlock,
    ) {
        let area = block.area();
        let mut vec = vec![false; area as usize];
        while let Some(p) = pixel_iterator.next() {
            assert!(block.contains(&p));
            let index = (p.x - block.min.x) + (p.y - block.min.y) * block.width();
            assert!(!vec[index as usize]);
            vec[index as usize] = true;
        }
        assert!(vec.into_iter().all(|v| v));
    }

    /// Tests that pixel iterator covers all pixels in a block
    #[proptest]
    fn pixel_iterator_covers_all(#[strategy(screen_block_strategy())] block: ScreenBlock) {
        check_pixel_iterator_covers_block(block.internal_points(), block);
    }

    /// Tests that pixel iterator is a well behaved exact length iterator
    #[proptest]
    fn pixel_iterator_exact_length(#[strategy(screen_block_strategy())] block: ScreenBlock) {
        check_exact_length(block.internal_points(), block.area() as usize);
    }

    /// Tests that sub blocks of a tile ordering when iterated over cover all pixels in a block
    #[proptest]
    fn tile_ordering_covers_all(
        #[strategy(screen_block_strategy())] block: ScreenBlock,
        tile_size_minus_one: u8,
    ) {
        check_pixel_iterator_covers_block(
            block
                .tile_ordering(NonZeroU32::new(tile_size_minus_one as u32 + 1).unwrap())
                .iter()
                .flat_map(|tile| tile.internal_points()),
            block,
        );
    }

    #[test]
    fn screen_block_is_empty() {
        assert!(!ScreenBlock::new([0, 0].into(), [10, 10].into()).is_empty());

        assert!(ScreenBlock::new([0, 0].into(), [0, 0].into()).is_empty());
        assert!(ScreenBlock::new([0, 0].into(), [10, 0].into()).is_empty());
        assert!(ScreenBlock::new([5, 5].into(), [10, 1].into()).is_empty());
    }
}
