use std::cmp;
use std::convert::TryInto;
use std::iter::FusedIterator;

/// A block of screen space. Can be iterated by pixels or by sub blocks.
#[derive(Copy, Clone, Debug)]
pub struct Block {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

impl Block {
    /// Create new block starting at 0, 0
    pub fn new(w: usize, h: usize) -> Block {
        Block { x: 0, y: 0, w: w, h: h }
    }

    /// Return true if the block contains no coordinates
    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    pub fn contains(&self, coord: (usize, usize)) -> bool {
        coord.0 >= self.x && coord.0 < (self.x + self.w) &&
        coord.1 >= self.y && coord.1 < (self.y + self.h)
    }

    /// Create an iterator over coordinates (x, y) pairs inside the block, 
    /// in C order (x changes first, then y)
    pub fn pixel_coordinates(&self) -> PixelCoordinates {
        if self.is_empty() {
            PixelCoordinates {
                x0: 1,
                x1: 0,
                y1: 0,

                x: 1,
                y: 1,
            }
        } else {
            PixelCoordinates {
                x0: self.x,
                x1: self.x + self.w - 1,
                y1: self.y + self.h - 1,

                x: self.x,
                y: self.y,
            }
        }
    }

    /// Create an iterator over sub-blocks in spiral order, starting in the middle of the block.
    /// Chunks are chunk_size * chunk_size large, except on the bottom and right side of the
    /// block, where they may be clipped if chunk size doesn't evenly divide block size.
    /// Chunk size must be laregr than zero. May fail if chunk size is 1 and block width or height
    /// are very large.
    pub fn spiral_chunks(self, chunk_size: usize) -> Result<SpiralChunks, &'static str> {
        if chunk_size == 0 {
            return Err("Chunk size can't be zero");
        }

        let w: isize = divide_round_up(self.w, chunk_size).try_into().or(Err("Width divided by block size must fit into isize"))?;
        let h: isize = divide_round_up(self.h, chunk_size).try_into().or(Err("Height divided by block size must fit into isize"))?;

        let x = w / 2;
        let y = h / 2;

        let dx = -((h - 2 * y) as isize); // dx will always be 0 or 1.
        let dy = -1 - dx;

        Ok(SpiralChunks {
            block: self,
            chunk_size: chunk_size,

            w: w,
            h: h,

            x: x,
            y: y,
            dx: dx,
            dy: dy,
            segment: 2,
            segment_remaining: 1,
            remaining: (w * h) as usize,
        })
    }

    /// Internal constructor for creating a sub-block from a block.
    /// Assumes that all the inputs are reasonable
    fn subblock(&self, block_x: usize, block_y: usize, chunk_size: usize) -> Block {
        let x = block_x * chunk_size;
        let y = block_y * chunk_size;
        Block {
            x: x + self.x,
            y: y + self.y,
            w: cmp::min(chunk_size, self.w - x),
            h: cmp::min(chunk_size, self.h - y)
        }
    }

}

#[derive(Copy,Clone,Debug)]
pub struct PixelCoordinates {
    x0: usize,
    x1: usize,
    y1: usize,

    x: usize,
    y: usize,
}

impl Iterator for PixelCoordinates {
    type Item = (usize, usize);

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }

    fn next(&mut self) -> Option<Self::Item> {
        if self.x > self.x1 {
            return None
        }

        let ret = (self.x, self.y);

        if self.x < self.x1 {
            self.x += 1;
        } else if self.y < self.y1 {
            self.x = self.x0;
            self.y += 1;
        } else {
            self.x = self.x1 + 1; // Mark as failed
        }

        Some(ret)
    }
}

impl ExactSizeIterator for PixelCoordinates {
    fn len(&self) -> usize {
        if self.x > self.x1 {
            0
        } else {
            (self.y1 - self.y) * (self.x1 - self.x0 + 1) + (self.x1 - self.x) + 1
        }
    }
}

impl FusedIterator for PixelCoordinates {}

/// Iterator over (mostly) square blocks within a rectangular box in spiral order.
#[derive(Copy,Clone,Debug)]
pub struct SpiralChunks {
    block: Block,
    chunk_size: usize,

    w: isize,
    h: isize,

    x: isize,
    y: isize,
    dx: isize,
    dy: isize,
    segment: usize,
    segment_remaining: isize,
    remaining: usize,
}

impl SpiralChunks {
    fn next_segment(&mut self) {
        let old_dx = self.dx;
        self.dx = self.dy;
        self.dy = -old_dx;

        self.segment += 1;
        self.segment_remaining = (self.segment / 2) as isize;
    }
}

impl Iterator for SpiralChunks {
    type Item = Block;

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }

    fn next(&mut self) -> Option<Block> {
        if self.remaining == 0 {
            return None
        }

        let ret = self.block.subblock(
            self.x as usize, self.y as usize,
            self.chunk_size);
        assert!(!ret.is_empty());

        if self.segment_remaining == 0 {
            self.next_segment();
        }

        let new_x = self.x + self.dx;
        let new_y = self.y + self.dy;
        self.segment_remaining -= 1;

        if (0..self.w).contains(&new_x) && (0..self.h).contains(&new_y) {
            // We're inside boundaries and can continue
            self.x = new_x;
            self.y = new_y;
        } else {
            // Got outside of the area.
            // In this case we don't move the cursor (don't use new_x and new_y) and instead
            // turn to new segment immediately.
            self.next_segment();

            // Then we skip the whole next segment (it would be outside the area anyway)
            self.x += self.segment_remaining * self.dx;
            self.y += self.segment_remaining * self.dy;

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
        self.remaining
    }
}

impl FusedIterator for SpiralChunks {}

fn divide_round_up(a: usize, b: usize) -> usize {
    let d = a / b;
    let m = a % b;
    d + ((m != 0) as usize)
}

#[cfg(test)]
mod test {
    use proptest::prelude::*;

    use super::*;

    fn abs_difference(x: usize, y: usize) -> usize {
        if x < y {
            y - x
        } else {
            x - y
        }
    }

    fn check_exact_length_internal<T: Iterator + ExactSizeIterator>(iterator: &T, expected_length: usize) {
        let len = iterator.len();
        assert_eq!(len, expected_length);
        let (min, max) = iterator.size_hint();
        assert_eq!(min, len);
        assert_eq!(max.unwrap(), len);
    }

    /// Goes through the whole iterator and checks that at every step iterator's size hint is equal
    /// to its reported length and equal to the expected number of elements.
    fn check_exact_length<T: Iterator + ExactSizeIterator>(mut iterator: T, expected_length: usize) {
        check_exact_length_internal(&iterator, expected_length);

        let mut count = 0usize;
        while let Some(_) = iterator.next() {
            count += 1;
            check_exact_length_internal(&iterator, expected_length - count);
        }
    }

    /// Check that all pixels in the block are covered by 
    fn check_pixel_iterator_covers_block<T: Iterator<Item = (usize, usize)>>(mut pixel_iterator: T, block: Block) {
        let mut vec = vec!(false; block.w * block.h);
        while let Some((x, y)) = pixel_iterator.next() {
            assert!(block.contains((x, y)));
            let index = (x - block.x) + (y - block.y) * block.w;
            assert!(!vec[index]);
            vec[index] = true;
        }
        assert!(vec.into_iter().all(|v| v));
    }

    proptest! {
        /// Tests that pixel iterator covers all pixels in a block
        #[test]
        fn pixel_iterator_covers_all(x in 0..100usize,
                                     y in 0..100usize,
                                     w in 0..100usize,
                                     h in 0..100usize) {
            let block = Block { x: x, y: y, w: w, h: h };
            check_pixel_iterator_covers_block(block.pixel_coordinates(), block);
        }

        /// Tests that pixel iterator is a well behaved exact length iterator
        #[test]
        fn pixel_iterator_exact_length(x in 0..100usize,
                                       y in 0..100usize,
                                       w in 0..100usize,
                                       h in 0..100usize) {
            let block = Block { x: x, y: y, w: w, h: h };
            check_exact_length(block.pixel_coordinates(), w * h);
        }

        /// Tests that sub blocks of a spiral chunk iterator when iterated over cover all pixels in
        /// a block
        #[test]
        fn spiral_iterator_covers_all(x in 0..100usize,
                                      y in 0..100usize,
                                      w in 0..100usize,
                                      h in 0..100usize,
                                      block_size in 1..10usize) {
            let block = Block { x: x, y: y, w: w, h: h };
            check_pixel_iterator_covers_block(block.spiral_chunks(block_size).
                                                    unwrap().
                                                    flat_map(|chunk| chunk.pixel_coordinates()),
                                              block);
        }

        /// Tests that the spiral iterator actually goes in a spiral.
        /// This test is not 100% robust, it only checs that we are going through the picture in
        /// squares of increasing size. The order hovewer is just a visual feature and if it looks
        /// good enough, then it's good enough.
        #[test]
        fn spiral_iterator_is_spiral(x in 0..100usize,
                                     y in 0..100usize,
                                     w in 0..100usize,
                                     h in 0..100usize,
                                     block_size in 1..10usize) {
            let block = Block { x: x, y: y, w: w, h: h };
            let mut it = block.spiral_chunks(block_size).unwrap();

            if let Some(first) = it.next() {
                let mut prev_distance = 0;
                for subblock in it {
                    let distance = cmp::max(abs_difference(first.x, subblock.x),
                                            abs_difference(first.y, subblock.y));
                    assert!(distance >= prev_distance);
                    prev_distance = distance;
                }
            }
        }

        /// Tests that the spiral iterator actually goes in a spiral.
        /// This test is not 100% robust, it only checs that we are going through the picture in
        /// squares of increasing size. The order hovewer is just a visual feature and if it looks
        /// good enough, then it's good enough.
        #[test]
        fn spiral_iterator_exact_length(x in 0..100usize,
                                        y in 0..100usize,
                                        w in 0..100usize,
                                        h in 0..100usize,
                                        block_size in 1..10usize) {
            let block = Block { x: x, y: y, w: w, h: h };
            let it = block.spiral_chunks(block_size).unwrap();

            check_exact_length(it, it.len()); // Assume that the initial length reported is correct
        }
    }

    #[test]
    fn single_pixel() {
        assert_eq!(Block::new(1, 1).pixel_coordinates().collect::<Vec<_>>(), [(0, 0)]);
    }
}
