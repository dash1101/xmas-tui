//! Things the villager can climb and stand on: roofs (their real outline,
//! slopes, chimneys and steeple), trees (up to the star), the station, lamp
//! posts, presents, the cocoa cabin and the snowman; and on the desktop,
//! the tops of the widgets. Each is the top edge of its shape, column by
//! column.

/// What a ledge is (some things care: the camera holds still on a widget,
/// which stays put on the screen while the village scrolls).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    House,
    Tree,
    Lamp,
    Station,
    Present,
    Shop,
    Snowman,
    /// A widget, on the screen (its columns are where it is this frame).
    Widget,
}

#[derive(Clone, Debug)]
pub struct Ledge {
    pub kind: Kind,
    /// First column, and the top edge of the shape at each column from
    /// there (in rows: half a row lower where its top cell is half empty).
    pub x0: i32,
    pub tops: Vec<Option<f32>>,
    /// Its foot (how far forward it stands): the nearest is climbed first.
    pub base: i32,
}

/// The tallest step the villager walks up or down without jumping.
pub const STEP: f32 = 2.5;

impl Ledge {
    /// A shape given by `solid(x, y)`, over columns x0..x1 and rows
    /// top..=base: its top edge.
    pub fn from_shape(kind: Kind, x0: i32, x1: i32, top: i32, base: i32, solid: impl Fn(i32, i32) -> bool) -> Option<Ledge> {
        let tops: Vec<Option<f32>> = (x0..x1).map(|x| (top..=base).find(|&y| solid(x, y)).map(|y| y as f32)).collect();
        tops.iter().any(|t| t.is_some()).then_some(Ledge { kind, x0, tops, base })
    }

    /// A shape given by its top edge in each column it covers.
    pub fn from_tops(kind: Kind, tops: &std::collections::BTreeMap<i32, f32>, base: i32) -> Option<Ledge> {
        let (&x0, _) = tops.first_key_value()?;
        let (&x1, _) = tops.last_key_value()?;
        Some(Ledge { kind, x0, tops: (x0..=x1).map(|x| tops.get(&x).copied()).collect(), base })
    }

    /// A flat top: columns x..x+w, top row `top`.
    pub fn flat(kind: Kind, x: i32, w: i32, top: i32, base: i32) -> Ledge {
        Ledge { kind, x0: x, tops: vec![Some(top as f32); w.max(1) as usize], base }
    }

    /// The row the villager's feet are on, standing on top at `x` (the
    /// cell above the shape; `x` in column floor(x)).
    pub fn feet(&self, x: f32) -> Option<f32> {
        let i = x.floor() as i32 - self.x0;
        if i < 0 {
            return None;
        }
        self.tops.get(i as usize).copied().flatten().map(|t| t - 1.0)
    }
}

/// Where a fall from row `from` to row `to` at column `x` lands on a ledge
/// (the highest one crossed), if it does: (ledge, feet row). `depth`: a
/// jump from the snowfield, from this row: it only lands on things standing
/// about as far forward (or on a widget).
pub fn landing(ledges: &[Ledge], x: f32, from: f32, to: f32, skip: Option<usize>, depth: Option<f32>) -> Option<(usize, f32)> {
    ledges
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != skip)
        .filter(|(_, l)| l.kind == Kind::Widget || depth.is_none_or(|d| (l.base as f32 - d).abs() <= 4.0))
        .filter_map(|(i, l)| l.feet(x).map(|f| (i, f)))
        .filter(|&(_, f)| from <= f + 0.01 && to >= f)
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// Something to climb at column `x` from row `y`: the nearest thing whose
/// top is above them. In the village that's from the back of the snowfield
/// (`back`), right up against it; a widget, from anywhere in front of it or
/// just under it (its `base` is its bottom row).
pub fn climbable(ledges: &[Ledge], x: f32, y: f32, back: bool) -> Option<(usize, f32)> {
    ledges
        .iter()
        .enumerate()
        .filter_map(|(i, l)| l.feet(x).map(|f| (i, f, l)))
        .filter(|&(_, f, l)| {
            f < y - 1.0
                && match l.kind {
                    Kind::Widget => y <= l.base as f32 + 6.0,
                    _ => back && l.base as f32 <= y + 1.0,
                }
        })
        .max_by_key(|&(_, _, l)| l.base)
        .map(|(i, f, _)| (i, f))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gable_roof_is_walked_over_its_slope() {
        // a roof: a triangle over columns 10..20, ridge at row 5, eaves at 9
        let roof = |x: i32, y: i32| y >= 5 + (x - 15).abs() * 4 / 5 && y <= 12;
        let l = Ledge::from_shape(Kind::House, 10, 21, 0, 12, roof).unwrap();
        assert_eq!(l.feet(15.0), Some(4.0));
        assert!(l.feet(11.0).unwrap() > l.feet(14.0).unwrap(), "the ridge isn't the top");
        assert_eq!(l.feet(9.0), None);
        // falling from above lands on it; walking past it doesn't
        let ls = vec![l, Ledge::flat(Kind::Lamp, 30, 2, 8, 20)];
        assert_eq!(landing(&ls, 15.0, 0.0, 6.0, None, None), Some((0, 4.0)));
        assert_eq!(landing(&ls, 15.0, 5.0, 6.0, None, None), None);
        assert_eq!(landing(&ls, 30.0, 0.0, 9.0, None, None).map(|l| l.0), Some(1));
        // a hop in the snow far in front of the lamp doesn't land on it
        assert_eq!(landing(&ls, 30.0, 0.0, 9.0, None, Some(40.0)), None);
        // climbing: from the snowfield in front, up onto the roof
        assert_eq!(climbable(&ls, 15.0, 16.0, true), Some((0, 4.0)));
        assert_eq!(climbable(&ls, 15.0, 16.0, false), None);
        assert_eq!(climbable(&ls, 25.0, 16.0, true), None);
        // a widget: from just under it
        let ls = vec![Ledge::flat(Kind::Widget, 40, 10, 20, 26)];
        assert_eq!(climbable(&ls, 44.0, 30.0, false), Some((0, 19.0)));
        assert_eq!(climbable(&ls, 44.0, 40.0, false), None);
    }
}
