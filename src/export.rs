//! Headless renderer: rasterizes the cell grid to a PNG exactly the way the
//! native window backend draws it, so frames can be inspected without
//! touching the live desktop.

use crate::frame::Cell;
use crate::raster::Raster;
use std::path::Path;

pub struct Exporter {
    raster: Raster,
    /// Pictures to draw over the cells (real icons), as the window does.
    pub pictures: Vec<crate::frame::Placed>,
}

impl Exporter {
    pub fn new() -> Self {
        crate::frame::set_pictures_ok(true);
        Exporter { raster: Raster::new(9, 19), pictures: vec![] }
    }

    /// The frame as packed RGB bytes: (pixels, width, height).
    pub fn rgb(&mut self, cells: &[Cell], w: usize, h: usize) -> (Vec<u8>, usize, usize) {
        let (cw, ch) = (self.raster.cw, self.raster.ch);
        let (iw, ih) = (w * cw, h * ch);
        let mut buf = vec![0u32; iw * ih];
        for cy in 0..h {
            for cx in 0..w {
                self.raster.draw_cell(&mut buf, iw, cx * cw, cy * ch, &cells[cy * w + cx]);
            }
        }
        for p in &self.pictures {
            p.pic.blit(&mut buf, iw, p.x as usize * cw, p.y as usize * ch, p.cols as usize * cw, p.rows as usize * ch);
        }
        let mut img = Vec::with_capacity(iw * ih * 3);
        for p in buf {
            img.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
        }
        (img, iw, ih)
    }

    pub fn render(&mut self, cells: &[Cell], w: usize, h: usize, path: &Path) -> std::io::Result<()> {
        let (img, iw, ih) = self.rgb(cells, w, h);
        let file = std::fs::File::create(path)?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), iw as u32, ih as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().map_err(std::io::Error::other)?;
        wr.write_image_data(&img).map_err(std::io::Error::other)?;
        Ok(())
    }
}
