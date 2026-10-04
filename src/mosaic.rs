use crate::editor::{EffectStrength, Point, Stroke};
use image::RgbaImage;

const SAMPLE_BLOCK: u32 = 4;

pub struct Mosaic {
    levels: [MosaicLevel; 4],
}

impl Mosaic {
    pub fn new(image: &RgbaImage) -> Self {
        let sums = BlockSums::new(image);
        Self {
            levels: EffectStrength::ALL
                .map(|strength| MosaicLevel::new(&sums, image.dimensions(), strength.block_size())),
        }
    }

    pub fn tiles_at(
        &self,
        stroke: &Stroke,
        offset: Point,
    ) -> impl Iterator<Item = (Point, Point, u32)> + '_ {
        self.levels[stroke.effect_strength.index()].tiles(stroke, offset)
    }
}

struct BlockSums {
    columns: usize,
    channels: Vec<[u32; 4]>,
}

impl BlockSums {
    fn new(image: &RgbaImage) -> Self {
        let columns = image.width().div_ceil(SAMPLE_BLOCK) as usize;
        let rows = image.height().div_ceil(SAMPLE_BLOCK) as usize;
        let mut channels = vec![[0; 4]; columns * rows];
        if columns > 0 {
            for (y, row) in image
                .as_raw()
                .chunks_exact(image.width() as usize * 4)
                .enumerate()
            {
                let offset = y / SAMPLE_BLOCK as usize * columns;
                for (x, block) in row.chunks(SAMPLE_BLOCK as usize * 4).enumerate() {
                    let sum = &mut channels[offset + x];
                    for pixel in block.as_chunks::<4>().0 {
                        for (sum, &channel) in sum.iter_mut().zip(pixel) {
                            *sum += u32::from(channel);
                        }
                    }
                }
            }
        }
        Self { columns, channels }
    }
}

struct MosaicLevel {
    block_size: u32,
    columns: u32,
    rows: u32,
    colors: Vec<u32>,
    width: u32,
    height: u32,
}

impl MosaicLevel {
    fn new(sums: &BlockSums, (width, height): (u32, u32), block_size: u32) -> Self {
        debug_assert_eq!(block_size % SAMPLE_BLOCK, 0);
        let columns = width.div_ceil(block_size);
        let rows = height.div_ceil(block_size);
        let mut colors = Vec::with_capacity((columns * rows) as usize);
        for row in 0..rows {
            for column in 0..columns {
                let left = column * block_size;
                let top = row * block_size;
                let right = (left + block_size).min(width);
                let bottom = (top + block_size).min(height);
                let mut channels = [0u32; 4];
                // Keep sums until the final level so shared blocks introduce no rounding error.
                for y in top / SAMPLE_BLOCK..bottom.div_ceil(SAMPLE_BLOCK) {
                    let offset = y as usize * sums.columns;
                    for x in left / SAMPLE_BLOCK..right.div_ceil(SAMPLE_BLOCK) {
                        for (sum, value) in
                            channels.iter_mut().zip(sums.channels[offset + x as usize])
                        {
                            *sum += value;
                        }
                    }
                }
                let count = (right - left) * (bottom - top);
                let [r, g, b, a] = channels.map(|value| value / count);
                colors.push((r << 24) | (g << 16) | (b << 8) | a);
            }
        }
        Self {
            block_size,
            columns,
            rows,
            colors,
            width,
            height,
        }
    }

    fn tiles(
        &self,
        stroke: &Stroke,
        offset: Point,
    ) -> impl Iterator<Item = (Point, Point, u32)> + '_ {
        let bounds = stroke.bounds();
        let left = (bounds.x + offset.x).max(0.0);
        let top = (bounds.y + offset.y).max(0.0);
        let right = (bounds.x + offset.x + bounds.width).min(self.width as f32);
        let bottom = (bounds.y + offset.y + bounds.height).min(self.height as f32);
        let block = self.block_size as f32;
        let rows = if self.columns == 0 || self.rows == 0 || right <= left || bottom <= top {
            0..0
        } else {
            (top / block).floor() as u32..(bottom / block).ceil() as u32
        };
        let columns = (left / block).floor() as u32..(right / block).ceil() as u32;
        rows.flat_map(move |row| {
            columns.clone().map(move |column| {
                (
                    Point::new(
                        (column as f32 * block).max(left) - offset.x,
                        (row as f32 * block).max(top) - offset.y,
                    ),
                    Point::new(
                        ((column + 1) as f32 * block).min(right) - offset.x,
                        ((row + 1) as f32 * block).min(bottom) - offset.y,
                    ),
                    self.colors[(row * self.columns + column) as usize],
                )
            })
        })
    }
}

#[cfg(test)]
mod tests;
