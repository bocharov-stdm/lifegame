//! Равномерная сетка для поиска соседей.
//!
//! Строится заново каждый тик сортировкой подсчётом: номер клетки для каждой
//! точки → сколько точек в клетке → префиксные суммы → раскладка. Всё за O(n),
//! а точки одной клетки лежат в памяти подряд вместе с копиями координат,
//! поэтому перебор соседей идёт по плотному массиву.
//!
//! Клетка фиксированного размера. Запрос берёт столько клеток, сколько
//! покрывает его собственный радиус, — в отличие от Python-версии, где клетка
//! равнялась самому большому радиусу в мире и одно дальнозоркое существо
//! раздувало её всем. Запрос возвращает НАДмножество: расстояние проверяет
//! вызывающий.
//!
//! Координаты — копия на момент постройки. Это безопасно, потому что в фазе,
//! которая спрашивает сетку, её точки не двигаются: растения не ходят вовсе, а
//! травоядные в проходе каннибализма уже стоят.
//! Живость (`alive`) вызывающий проверяет по самим сущностям — она меняется
//! прямо в фазе.

use crate::space::Space;

#[derive(Clone, Debug, Default)]
pub struct Grid {
    inv: f64,
    cols: usize,
    rows: usize,
    /// start[c]..start[c+1] — точки клетки c в idx/xs/ys.
    start: Vec<u32>,
    idx: Vec<u32>,
    xs: Vec<f64>,
    ys: Vec<f64>,
    // рабочие буферы постройки — живут между тиками, чтобы не выделять память
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

    /// Разложить точки по клеткам. Буферы переиспользуются между тиками.
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
        // раскладка: next[c] — куда класть следующую точку клетки c
        self.next.clear();
        self.next.extend_from_slice(&self.start);
        for (i, &c) in self.cell_of.iter().enumerate() {
            let at = self.next[c as usize] as usize;
            self.next[c as usize] += 1;
            self.idx[at] = i as u32;
            (self.xs[at], self.ys[at]) = self.pts[i];
        }
    }

    /// Все точки из клеток, накрывающих квадрат [x-r, x+r] x [y-r, y+r]:
    /// f(индекс, x, y). Надмножество круга радиуса r.
    #[inline]
    pub fn for_each_near(&self, x: f64, y: f64, r: f64, mut f: impl FnMut(usize, f64, f64)) {
        if self.idx.is_empty() {
            return;
        }
        let (c0, c1) = (self.col(x - r), self.col(x + r));
        let (r0, r1) = (self.row(y - r), self.row(y + r));
        for row in r0..=r1 {
            let base = row * self.cols;
            // клетки одной строки идут подряд — один непрерывный отрезок
            let from = self.start[base + c0] as usize;
            let to = self.start[base + c1 + 1] as usize;
            for k in from..to {
                f(self.idx[k] as usize, self.xs[k], self.ys[k]);
            }
        }
    }
}
