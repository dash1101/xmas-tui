//! The widgets' looks, which the scene's own panels (conversations, the
//! games' table) are drawn in too; and the rectangle they take.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}

#[path = "../../../src/widgets/looks.rs"]
pub mod looks;
