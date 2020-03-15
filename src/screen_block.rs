use euclid::*;
use std::cmp;
use std::iter::FusedIterator;

pub struct ScreenSpace;
pub type PixelPosition = Point2D<u32, ScreenSpace>;
pub type ScreenSize = Size2D<u32, ScreenSpace>;
pub type ScreenBlock = Box2D<u32, ScreenSpace>;

/// Coordinates of chunks in the image. The scaling factor is potentially different for every chunk
/// iterator.
struct ChunkSpace;

pub trait ScreenBlockExt {
    fn internal_points(&self) -> InternalPoints;
    fn spiral_chunks(&self, chunk_size: u32) -> SpiralChunks;
}

impl ScreenBlockExt for ScreenBlock {
    /// Create an iterator over coordinates (x, y) pairs inside the block,
    /// in C order (x changes first, then y)
    fn internal_points(&self) -> InternalPoints {
        if self.is_empty_or_negative() {
            InternalPoints::empty()
        } else {
            InternalPoints {
                min_x: self.min.x,
                max: self.max,

                cursor: self.min,
            }
        }
    }

    /// Create an iterator over sub blocks in (roughly) spiral order, starting in the middle of the block.
    /// Chunks are chunk_size * chunk_size large, except on the bottom and right side of the
    /// block, where they may be clipped if chunk size doesn't evenly divide block size.
    /// Chunk size must be larger than zero. May panic if chunk size is small (1 or 2) and block
    /// size is very large.
    /// Chunk size must be non zero.
    fn spiral_chunks(&self, chunk_size: u32) -> SpiralChunks {
        assert!(chunk_size > 0);

        if self.is_empty_or_negative() {
            return SpiralChunks::empty();
        }

        let chunk_scale = Scale::new(chunk_size);
        let size = divide_round_up(self.size(), chunk_scale).cast::<i32>();
        let cursor = Box2D::from(size).center();

        let dx = 2 * cursor.y - size.height;
        debug_assert!(dx == 0 || dx == -1);
        let direction = Vector2D::new(dx, -1 - dx);

        SpiralChunks {
            block: *self,

            chunk_scale,
            size,
            cursor,
            direction,

            segment: 2,
            segment_remaining: 1,
            remaining: size.area() as u32,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct InternalPoints {
    min_x: u32, // Unfortunately this can't easily be Length :-( TODO: Fix this in euclid?
    max: PixelPosition,

    cursor: PixelPosition,
}

impl InternalPoints {
    // Construct an iterator over internal points that returns no points
    fn empty() -> Self {
        InternalPoints {
            min_x: 1,
            max: Point2D::zero(),

            cursor: Point2D::zero(),
        }
    }
}

impl Iterator for InternalPoints {
    type Item = PixelPosition;

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
            let whole_rows = Box2D::new(point2(self.min_x, self.cursor.y + 1), self.max);
            let current_row = Box2D::new(self.cursor, point2(self.max.x, self.cursor.y + 1));
            (whole_rows.area() + current_row.area()) as usize
        }
    }
}

impl FusedIterator for InternalPoints {}

/// Iterator over (mostly) square blocks within a rectangular box in spiral order.
#[derive(Copy, Clone, Debug)]
pub struct SpiralChunks {
    block: ScreenBlock,

    chunk_scale: Scale<u32, ChunkSpace, ScreenSpace>,
    size: Size2D<i32, ChunkSpace>,
    cursor: Point2D<i32, ChunkSpace>,
    direction: Vector2D<i32, ChunkSpace>,

    segment: u32,
    segment_remaining: i32,
    remaining: u32,
}

impl SpiralChunks {
    /// Constructs an iterator that returns no blocks.
    fn empty() -> SpiralChunks {
        SpiralChunks {
            block: Box2D::zero(),

            chunk_scale: Scale::new(0),
            size: Size2D::zero(),
            cursor: Point2D::zero(),
            direction: vec2(1, 0),

            segment: 0,
            segment_remaining: 0,
            remaining: 0,
        }
    }

    /// Moves to next segment of the spiral (turns 90 degrees and calculates new segment legnth).
    fn next_segment(&mut self) {
        self.direction = vec2(self.direction.y, -self.direction.x);
        self.segment += 1;
        self.segment_remaining = (self.segment / 2) as i32;
    }

    /// Returns a new screen block that corresponds to the current iterator position.
    fn current_block(&self) -> ScreenBlock {
        let min = self.block.min + self.cursor.to_vector().cast::<u32>() * self.chunk_scale;
        let max = min + vec2(1, 1) * self.chunk_scale;
        let ret = ScreenBlock {
            min,
            max: point2(
                cmp::min(self.block.max.x, max.x),
                cmp::min(self.block.max.y, max.y),
            ),
        };
        debug_assert!(self.block.contains_box(&ret));
        debug_assert!(!ret.is_empty_or_negative());
        ret
    }
}

impl Iterator for SpiralChunks {
    type Item = ScreenBlock;

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.remaining as usize;
        (remaining, Some(remaining))
    }

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let ret = self.current_block();

        if self.segment_remaining == 0 {
            self.next_segment();
        }

        let new_cursor = self.cursor + self.direction;
        self.segment_remaining -= 1;

        if Box2D::from(self.size).contains(new_cursor) {
            // We're inside boundaries and can continue
            self.cursor = new_cursor;
        } else {
            // Got outside of the area.
            // In this case we don't move the cursor (don't use new_x and new_y) and instead
            // turn to new segment immediately.
            self.next_segment();

            // Then we skip the whole next segment (it would be outside the area anyway)
            self.cursor += self.direction * self.segment_remaining;

            // And finally we turn to the next segment which is inside the area
            // Note that segment_remaining for this one is wrong (since we skipped
            // its part outside of the screen, but we will terminate through this branch
            // of the iterator again, so it's not a problem and we don't need to fix it.
            self.next_segment();
        }

        self.remaining -= 1;

        Some(ret)
    }
}

impl ExactSizeIterator for SpiralChunks {
    fn len(&self) -> usize {
        self.remaining as usize
    }
}

impl FusedIterator for SpiralChunks {}

fn divide_round_up(
    a: ScreenSize,
    b: Scale<u32, ChunkSpace, ScreenSpace>,
) -> Size2D<u32, ChunkSpace> {
    let div: Size2D<u32, ChunkSpace> = a / b;
    let need_round_up = a.not_equal(div * b);

    div + need_round_up.select_size(Size2D::new(1, 1), Size2D::zero())
}

#[cfg(test)]
mod test {
    use super::*;
    use proptest::prelude::*;
    use proptest_attr_macro::proptest;

    #[derive(Copy, Clone, Debug)]
    struct ScreenBlockWrapper(ScreenBlock);

    impl std::ops::Deref for ScreenBlockWrapper {
        type Target = ScreenBlock;
        fn deref(&self) -> &ScreenBlock {
            &self.0
        }
    }

    impl Arbitrary for ScreenBlockWrapper {
        type Parameters = ();
        type Strategy = proptest::strategy::BoxedStrategy<Self>;
        fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
            const RANGE: std::ops::Range<u32> = 0..100u32;
            (RANGE, RANGE, RANGE, RANGE)
                .prop_map(|coords| {
                    ScreenBlockWrapper(euclid::Box2D::new(
                        euclid::Point2D::new(coords.0, coords.1),
                        euclid::Point2D::new(coords.2, coords.3),
                    ))
                })
                .boxed()
        }
    }

    fn abs_difference(x: u32, y: u32) -> u32 {
        if x < y {
            y - x
        } else {
            x - y
        }
    }

    fn safe_area(block: ScreenBlock) -> u32 {
        if block.is_empty_or_negative() {
            0
        } else {
            block.area()
        }
    }

    fn check_exact_length_internal<T: Iterator + ExactSizeIterator>(
        iterator: &T,
        expected_length: usize,
    ) {
        assert_eq!(iterator.len(), expected_length);
        let (min, max) = iterator.size_hint();
        assert_eq!(min, expected_length);
        assert_eq!(max.unwrap(), expected_length);
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
    fn check_pixel_iterator_covers_block<T: Iterator<Item = PixelPosition>>(
        mut pixel_iterator: T,
        block: ScreenBlock,
    ) {
        let area = safe_area(block);
        let mut vec = vec![false; area as usize];
        while let Some(p) = pixel_iterator.next() {
            assert!(block.contains(p));
            let index = (p.x - block.min.x) + (p.y - block.min.y) * block.width();
            assert!(!vec[index as usize]);
            vec[index as usize] = true;
        }
        assert!(vec.into_iter().all(|v| v));
    }

    /// Tests that pixel iterator covers all pixels in a block
    #[proptest]
    fn pixel_iterator_covers_all(block: ScreenBlockWrapper) {
        check_pixel_iterator_covers_block(block.internal_points(), *block);
    }

    /// Tests that pixel iterator is a well behaved exact length iterator
    #[proptest]
    fn pixel_iterator_exact_length(block: ScreenBlockWrapper) {
        check_exact_length(block.internal_points(), safe_area(*block) as usize);
    }

    /// Tests that sub blocks of a spiral chunk iterator when iterated over cover all pixels in
    /// a block
    #[proptest]
    fn spiral_iterator_covers_all(block: ScreenBlockWrapper, chunk_size_minus_one: u8) {
        check_pixel_iterator_covers_block(
            block
                .spiral_chunks(chunk_size_minus_one as u32 + 1)
                .flat_map(|chunk| chunk.internal_points()),
            *block,
        );
    }

    /// Tests that the spiral iterator actually goes in a spiral.
    /// This test is not 100% robust, it only checs that we are going through the picture in
    /// squares of increasing size. The order hovewer is just a visual feature and if it looks
    /// good enough, then it's good enough.
    #[proptest]
    fn spiral_iterator_is_spiral(block: ScreenBlockWrapper, chunk_size_minus_one: u8) {
        let mut it = block.spiral_chunks(chunk_size_minus_one as u32 + 1);

        if let Some(first) = it.next() {
            let mut prev_distance = 0;
            for subblock in it {
                let distance = cmp::max(
                    abs_difference(first.min.x, subblock.min.x),
                    abs_difference(first.min.y, subblock.min.y),
                );
                assert!(distance >= prev_distance);
                prev_distance = distance;
            }
        }
    }

    /// Tests that pixel iterator is a well behaved exact length iterator
    #[proptest]
    fn spiral_iterator_exact_length(block: ScreenBlockWrapper, chunk_size_minus_one: u8) {
        let it = block.spiral_chunks(chunk_size_minus_one as u32 + 1);
        check_exact_length(it, it.len()); // Using first reported length as a baseline, because it's easy
    }

    #[proptest]
    #[should_panic]
    fn zero_sized_chunks(block: ScreenBlockWrapper) {
        block.spiral_chunks(0);
    }
}
