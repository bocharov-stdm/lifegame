//! A uniform grid for the neighbour search.
//!
//! Rebuilt every tick by a counting sort: the cell number of each point → how many points are in
//! a cell → prefix sums → the layout. All in O(n), and the points of one cell lie in memory in
//! a row together with copies of the coordinates, so going through the neighbours runs over a
//! dense array.
//!
//! The cell has a fixed size. A query takes as many cells as its own radius covers — unlike the
//! Python version, where the cell equalled the biggest radius in the world and one far-sighted
//! creature blew it up for everyone. A query returns a SUPERset: the caller checks the distance.
//!
//! The coordinates are a copy as of the build. This is safe because in the phase that asks the
//! grid its points do not move: plants do not walk at all, and creatures in the fight phase
//! already stand.
//! Aliveness (`alive`) the caller checks on the entities themselves — it changes right in the phase.

use crate::space::Space;

#[derive(Clone, Debug, Default)]
pub struct Grid {
    cell: f64,
    inv: f64,
    cols: usize,
    rows: usize,
    /// start[c]..start[c+1] — the points of cell c in idx/xs/ys.
    start: Vec<u32>,
    idx: Vec<u32>,
    xs: Vec<f64>,
    ys: Vec<f64>,
    // the working buffers of the build — they live between ticks so as not to allocate memory
    cell_of: Vec<u32>,
    next: Vec<u32>,
    pts: Vec<(f64, f64)>,
}

impl Grid {
    pub fn new(cell: f64) -> Self {
        assert!(cell > 0.0, "клетка сетки должна быть > 0");
        Grid { cell, inv: 1.0 / cell, ..Default::default() }
    }

    #[inline]
    fn col(&self, x: f64) -> usize {
        ((x * self.inv).max(0.0) as usize).min(self.cols - 1)
    }

    #[inline]
    fn row(&self, y: f64) -> usize {
        ((y * self.inv).max(0.0) as usize).min(self.rows - 1)
    }

    /// Lay the points out by cells. The buffers are reused between ticks.
    pub fn rebuild(&mut self, space: &Space, points: impl ExactSizeIterator<Item = (f64, f64)>) {
        self.cols = ((space.width * self.inv).ceil() as usize).max(1);
        self.rows = ((space.height * self.inv).ceil() as usize).max(1);
        let cells = self.cols * self.rows;
        let n = points.len();

        self.start.clear();
        self.start.resize(cells + 1, 0);
        self.cell_of.clear();
        self.xs.clear();
        self.ys.clear();
        self.xs.resize(n, 0.0);
        self.ys.resize(n, 0.0);
        self.idx.clear();
        self.idx.resize(n, 0);

        self.pts.clear();
        for (x, y) in points {
            let c = (self.row(y) * self.cols + self.col(x)) as u32;
            self.cell_of.push(c);
            self.start[c as usize + 1] += 1;
            self.pts.push((x, y));
        }
        for c in 0..cells {
            self.start[c + 1] += self.start[c];
        }
        // the layout: next[c] — where to put the next point of cell c
        self.next.clear();
        self.next.extend_from_slice(&self.start);
        for (i, &c) in self.cell_of.iter().enumerate() {
            let at = self.next[c as usize] as usize;
            self.next[c as usize] += 1;
            self.idx[at] = i as u32;
            (self.xs[at], self.ys[at]) = self.pts[i];
        }
    }

    /// All points from the cells covering the square [x-r, x+r] x [y-r, y+r]:
    /// f(index, x, y). A superset of the circle of radius r.
    #[inline]
    pub fn for_each_near(&self, x: f64, y: f64, r: f64, mut f: impl FnMut(usize, f64, f64)) {
        if self.idx.is_empty() {
            return;
        }
        let (c0, c1) = (self.col(x - r), self.col(x + r));
        let (r0, r1) = (self.row(y - r), self.row(y + r));
        for row in r0..=r1 {
            let base = row * self.cols;
            // the cells of one row go in a row — one continuous stretch
            let from = self.start[base + c0] as usize;
            let to = self.start[base + c1 + 1] as usize;
            for k in from..to {
                f(self.idx[k] as usize, self.xs[k], self.ys[k]);
            }
        }
    }

    /// The nearest point strictly closer than √`r2` that `keep` accepts, as (index, x, y); of
    /// equally near ones the first in `for_each_near`'s order — exactly what a scan of its square
    /// would find. The cells are searched in rings out from the point's own, and the search stops
    /// once no cell farther out can hold a nearer point: near food costs a cell or nine, not the
    /// whole square of sight.
    #[inline]
    pub fn nearest(
        &self,
        x: f64,
        y: f64,
        r2: f64,
        mut keep: impl FnMut(usize, f64, f64) -> bool,
    ) -> Option<(usize, f64, f64)> {
        if self.idx.is_empty() {
            return None;
        }
        let r = r2.sqrt();
        let (c0, c1) = (self.col(x - r), self.col(x + r));
        let (r0, r1) = (self.row(y - r), self.row(y + r));
        let (cx, cy) = (self.col(x), self.row(y));
        // (d², position in the layout): the layout's order is the square scan's order
        let mut best: Option<(f64, usize)> = None;
        let mut visit = |cell: usize, best: &mut Option<(f64, usize)>| {
            for k in self.start[cell] as usize..self.start[cell + 1] as usize {
                let (px, py) = (self.xs[k], self.ys[k]);
                let (dx, dy) = (px - x, py - y);
                let d2 = dx * dx + dy * dy;
                if d2 < r2
                    && best.is_none_or(|(b, bk)| d2 < b || (d2 == b && k < bk))
                    && keep(self.idx[k] as usize, px, py)
                {
                    *best = Some((d2, k));
                }
            }
        };
        let rings = (cx - c0).max(c1 - cx).max(cy - r0).max(r1 - cy);
        for ring in 0..=rings {
            let (lo_r, hi_r) = (cy.saturating_sub(ring).max(r0), (cy + ring).min(r1));
            let (lo_c, hi_c) = (cx.saturating_sub(ring).max(c0), (cx + ring).min(c1));
            for row in lo_r..=hi_r {
                let base = row * self.cols;
                if row + ring == cy || row == cy + ring {
                    (lo_c..=hi_c).for_each(|c| visit(base + c, &mut best));
                } else {
                    if cx >= ring && cx - ring >= c0 {
                        visit(base + cx - ring, &mut best);
                    }
                    if ring > 0 && cx + ring <= c1 {
                        visit(base + cx + ring, &mut best);
                    }
                }
            }
            // how near the next ring can come: the edge of the block of rings searched so far, with a
            // margin for how a coordinate falls into a cell
            let Some((d2, _)) = best else { continue };
            let edge = |c: usize| c as f64 * self.cell;
            let gap = (x - edge(cx.saturating_sub(ring)))
                .min(edge(cx + ring + 1) - x)
                .min(y - edge(cy.saturating_sub(ring)))
                .min(edge(cy + ring + 1) - y)
                - 1e-6 * self.cell;
            if gap > 0.0 && d2 < gap * gap {
                break;
            }
        }
        best.map(|(_, k)| (self.idx[k] as usize, self.xs[k], self.ys[k]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;
    use crate::space::{Shape, Space};

    /// The ring search finds exactly what a scan of the whole square finds, ties included: points
    /// on a lattice, many at equal distances, some refused by the filter.
    #[test]
    fn nearest_matches_a_full_scan() {
        let space = Space::new(1.0, Shape::R3x2);
        let mut rng = Rng::new(5);
        for lattice in [false, true] {
            let points: Vec<(f64, f64)> = (0..3000)
                .map(|_| {
                    let (x, y) = (rng.uniform(0.0, space.width), rng.uniform(0.0, space.height));
                    if lattice { ((x / 64.0).round() * 64.0, (y / 64.0).round() * 64.0) } else { (x, y) }
                })
                .collect();
            let mut grid = Grid::new(256.0);
            grid.rebuild(&space, points.iter().copied());
            for q in 0..2000 {
                let (x, y) =
                    (rng.uniform(-50.0, space.width + 50.0), rng.uniform(-50.0, space.height + 50.0));
                let (x, y) = if lattice && q % 2 == 0 {
                    ((x / 32.0).round() * 32.0, (y / 32.0).round() * 32.0)
                } else {
                    (x, y)
                };
                let r = [1e-4, 30.0, 256.0, 400.0, 1300.0][q % 5];
                let keep = |i: usize, _: f64, _: f64| !i.is_multiple_of(7);
                let mut scan: Option<(usize, f64, f64, f64)> = None;
                grid.for_each_near(x, y, r, |i, px, py| {
                    let d2 = (px - x).powi(2) + (py - y).powi(2);
                    if keep(i, px, py) && d2 < scan.map_or(r * r, |b| b.3) {
                        scan = Some((i, px, py, d2));
                    }
                });
                assert_eq!(
                    grid.nearest(x, y, r * r, keep),
                    scan.map(|(i, px, py, _)| (i, px, py)),
                    "({x}, {y}) r {r}"
                );
            }
        }
    }
}
