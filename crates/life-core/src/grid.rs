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
        Grid { inv: 1.0 / cell, ..Default::default() }
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
}
