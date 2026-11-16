//! Things the villager can climb and stand on: roofs (their real outline,
//! slopes, chimneys and steeple), trees (up to the star), the station, lamp
//! posts, presents, the cocoa cabin and the snowman; and on the desktop,
//! the tops of the widgets and of the open windows. Each is the top edge of
//! its shape, column by column.

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
    /// An open window's top edge (the part of it that shows).
    Window,
}

#[derive(Clone, Debug)]
pub struct Ledge {
    pub kind: Kind,
    /// Which widget or window (0: part of the village): the ledges on the
    /// screen are made afresh every frame, and someone up on one keeps to it.
    pub id: u64,
    /// First column, and the top edge of the shape at each column from
    /// there (in rows: half a row lower where its top cell is half empty).
    pub x0: i32,
    pub tops: Vec<Option<f32>>,
    /// Its foot (how far forward it stands): the nearest is climbed first.
    pub base: i32,
}

/// The tallest step the villager walks up or down without jumping.
pub const STEP: f32 = 2.5;
/// On a tree: the tallest scramble up (or down) from branch to branch.
pub const TREE_STEP: f32 = 8.0;

impl Ledge {
    /// A shape given by `solid(x, y)`, over columns x0..x1 and rows
    /// top..=base: its top edge.
    pub fn from_shape(kind: Kind, x0: i32, x1: i32, top: i32, base: i32, solid: impl Fn(i32, i32) -> bool) -> Option<Ledge> {
        let tops: Vec<Option<f32>> = (x0..x1).map(|x| (top..=base).find(|&y| solid(x, y)).map(|y| y as f32)).collect();
        tops.iter().any(|t| t.is_some()).then_some(Ledge { kind, id: 0, x0, tops, base })
    }

    /// A shape given by its top edge in each column it covers.
    pub fn from_tops(kind: Kind, tops: &std::collections::BTreeMap<i32, f32>, base: i32) -> Option<Ledge> {
        let (&x0, _) = tops.first_key_value()?;
        let (&x1, _) = tops.last_key_value()?;
        Some(Ledge { kind, id: 0, x0, tops: (x0..=x1).map(|x| tops.get(&x).copied()).collect(), base })
    }

    /// A flat top: columns x..x+w, top row `top`.
    pub fn flat(kind: Kind, x: i32, w: i32, top: i32, base: i32) -> Ledge {
        Ledge { kind, id: 0, x0: x, tops: vec![Some(top as f32); w.max(1) as usize], base }
    }

    /// A flat top on the screen (a widget's, a window's): over columns
    /// x0..x1 (the cells wholly on it), its edge at row `top` (which can
    /// fall part way down a cell).
    pub fn edge(kind: Kind, id: u64, x0: f32, x1: f32, top: f32, base: i32) -> Option<Ledge> {
        let (a, b) = (x0.ceil() as i32, x1.floor() as i32);
        (b > a).then(|| Ledge { kind, id, x0: a, tops: vec![Some(top); (b - a) as usize], base })
    }

    /// The column of its highest point nearest `x` (the ridge of a roof, a
    /// tree's star): where W heads, up on it.
    pub fn peak(&self, x: f32) -> Option<f32> {
        let top = self.tops.iter().flatten().fold(f32::MAX, |a, &b| a.min(b));
        (0..self.tops.len())
            .filter(|&i| self.tops[i] == Some(top))
            .map(|i| (self.x0 + i as i32) as f32 + 0.5)
            .min_by(|a, b| (a - x).abs().total_cmp(&(b - x).abs()))
    }

    /// On the screen rather than in the village (it stays put while the
    /// village scrolls by).
    pub fn on_screen(&self) -> bool {
        matches!(self.kind, Kind::Widget | Kind::Window)
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

/// How far below the top of something a villager on the way down can be
/// and still land on it (a roof sloping up under them as they steer along
/// it: they catch the edge).
pub const CATCH: f32 = 1.6;

/// Standing at about the depth of something in the village (`d`: how far in
/// front of its foot, in rows): near enough to leap up onto it, or to land
/// on it from a jump. Things a little behind (the houses, at the back of the
/// snowfield) or a little in front (a tree, a present) count; something far
/// behind (the station, over the tracks) doesn't.
pub fn at_depth(d: f32) -> bool {
    (-2.5..=5.5).contains(&d)
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
        .filter(|(_, l)| l.on_screen() || depth.is_none_or(|d| at_depth(d - l.base as f32)))
        .filter_map(|(i, l)| l.feet(x).map(|f| (i, f)))
        .filter(|&(_, f)| from <= f + CATCH && to >= f)
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// Something to leap up onto from column `x`, row `y`, at most `reach` rows
/// up: in the village, the nearest thing standing at about the villager's
/// depth (see `at_depth`); a widget or window, from anywhere in front of it
/// or just under it (its `base` is its bottom row). Returns (ledge, feet
/// row, the column to come down on): the one underfoot, or if that's out of
/// reach or a step off the edge, the nearest one that isn't (a lower branch
/// of a tall tree).
pub fn climbable(ledges: &[Ledge], x: f32, y: f32, reach: f32) -> Option<(usize, f32, f32)> {
    let mut best: Option<(usize, f32, f32, i32)> = None;
    for (i, l) in ledges.iter().enumerate() {
        let near = if l.on_screen() { y <= l.base as f32 + 6.0 } else { at_depth(y - l.base as f32) };
        let over = x >= l.x0 as f32 - 2.0 && x < (l.x0 + l.tops.len() as i32) as f32 + 2.0;
        if !near || !over {
            continue;
        }
        // how far along to look for a foothold: a step or two, or on a tree
        // across to its lower branches
        let span = if l.kind == Kind::Tree { l.tops.len() as i32 } else { 2 };
        let ok = |f: f32| f < y - 1.0 && y - f <= reach;
        let found = (0..=span).flat_map(|d| [d, -d]).find_map(|d| {
            let cx = x.floor() + d as f32 + 0.5;
            l.feet(cx).filter(|&f| ok(f)).map(|f| (f, cx))
        });
        if let Some((f, cx)) = found {
            // something they're standing in front of (rather than something
            // standing a little in front of them); the nearest of those;
            // then the one right underfoot
            let behind = (l.on_screen() || l.base as f32 <= y) as i32;
            let key = behind * 1_000_000 + l.base * 1000 - (cx - x).abs().round() as i32;
            if best.is_none_or(|b| key > b.3) {
                best = Some((i, f, cx, key));
            }
        }
    }
    best.map(|(i, f, cx, _)| (i, f, cx))
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
        assert_eq!(landing(&ls, 15.0, 6.0, 7.0, None, None), None);
        assert_eq!(landing(&ls, 30.0, 0.0, 9.0, None, None).map(|l| l.0), Some(1));
        // a hop in the snow far in front of the lamp doesn't land on it
        assert_eq!(landing(&ls, 30.0, 0.0, 9.0, None, Some(40.0)), None);
        // climbing: from the snowfield in front, up onto the roof (not from
        // far in front of it, nor from beside it)
        assert_eq!(climbable(&ls, 15.0, 15.0, 30.0).map(|c| (c.0, c.1)), Some((0, 4.0)));
        assert_eq!(climbable(&ls, 15.0, 22.0, 30.0), None);
        assert_eq!(climbable(&ls, 25.0, 15.0, 30.0), None);
        // just off the end of the eaves: onto the end of it
        assert_eq!(climbable(&ls, 9.5, 14.0, 30.0).map(|c| (c.0, c.2)), Some((0, 10.5)));
        // a widget: from just under it
        let ls = vec![Ledge::flat(Kind::Widget, 40, 10, 20, 26)];
        assert_eq!(climbable(&ls, 44.0, 30.0, 30.0).map(|c| (c.0, c.1)), Some((0, 19.0)));
        assert_eq!(climbable(&ls, 44.0, 40.0, 30.0), None);
    }

    #[test]
    fn a_tall_tree_is_leapt_onto_a_branch_in_reach_and_the_station_isnt() {
        // a tree: a cone 40 rows tall over columns 0..41, standing at row 60
        let tree = |x: i32, y: i32| y >= 20 + (x - 20).abs() * 2 && y <= 60;
        let ls = vec![Ledge::from_shape(Kind::Tree, 0, 41, 0, 60, tree).unwrap(), Ledge::flat(Kind::Station, 60, 20, 40, 47)];
        // at the trunk, the star's out of reach: the nearest branch that isn't
        let (i, f, cx) = climbable(&ls, 20.5, 60.0, 24.0).unwrap();
        assert_eq!(i, 0);
        assert!(60.0 - f <= 24.0 && f < 59.0, "{f}");
        assert!((cx - 20.5).abs() >= 7.0 && (cx - 20.5).abs() <= 10.0, "{cx}");
        // the station, far behind over the tracks: not from the snowfield
        assert_eq!(climbable(&ls, 70.0, 60.0, 24.0), None);
        // coming down onto a slope rising under the villager as they steer
        // along: caught, not fallen through
        let f30 = ls[0].feet(30.5).unwrap();
        assert_eq!(landing(&ls, 30.5, f30 + 1.0, f30 + 1.5, None, None).map(|l| l.0), Some(0));
    }
}
