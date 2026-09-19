//! История партии для графиков. Порт `app/history.py` (тег python-final).
//!
//! Рядов у каждой величины два:
//! - «недавнее» — последние `RECENT` точек: крупно видно колебания
//!   «хищник — жертва»;
//! - «вся партия» — когда точек больше `FULL`, ряд прореживается вдвое (каждая
//!   вторая точка), а шаг записи удваивается. Память ограничена при любой
//!   длине партии.
//!
//! Точки снимает поток симуляции (ему видно каждый тик), а хранит окно.

use std::collections::VecDeque;

use life_core::genome::{predator, vegetarian};

use life_sim::observe::{GeneStat, Snapshot};

pub const RECENT: usize = 300;
pub const FULL: usize = 600;

/// Точка графика численностей. Численности — средние за `DIVIDE_PERIOD`
/// тиков: существа делятся разом раз в период, и мгновенные числа рисуют
/// пилу, за которой не видно самих колебаний.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub tick: u64,
    pub plants: f64,
    pub vegetarians: f64,
    pub predators: f64,
    /// Средний геном травоядных; None — травоядных нет.
    pub genom: Option<[f64; vegetarian::N]>,
}

/// Ряд с окном недавнего и прореженной всей партией.
#[derive(Clone, Debug)]
pub struct Series<T> {
    recent: VecDeque<T>,
    full: Vec<T>,
    /// В `full` лежит каждая stride-я точка.
    stride: usize,
    count: usize,
}

impl<T: Clone> Default for Series<T> {
    fn default() -> Self {
        Series { recent: VecDeque::with_capacity(RECENT), full: Vec::new(), stride: 1, count: 0 }
    }
}

impl<T: Clone> Series<T> {
    pub fn push(&mut self, point: T) {
        if self.recent.len() == RECENT {
            self.recent.pop_front();
        }
        self.recent.push_back(point.clone());
        if self.count.is_multiple_of(self.stride) {
            self.full.push(point);
            if self.full.len() > FULL {
                // full[i] — точка номер i * stride, поэтому каждая вторая — ровно
                // точки с номерами, кратными 2 * stride
                self.full = self.full.iter().step_by(2).cloned().collect();
                self.stride *= 2;
            }
        }
        self.count += 1;
    }

    /// Точки для графика: вся партия или последнее окно. Последняя точка есть
    /// всегда: при прореживании она могла не попасть в «всю партию». Ссылками:
    /// срезы мира крупные, и копировать сотни их на каждый кадр окна незачем.
    pub fn points(&self, whole: bool) -> Vec<&T> {
        if !whole {
            return self.recent.iter().collect();
        }
        let mut points: Vec<&T> = self.full.iter().collect();
        if !self.count.saturating_sub(1).is_multiple_of(self.stride)
            && let Some(last) = self.recent.back()
        {
            points.push(last);
        }
        points
    }

    pub fn last(&self) -> Option<&T> {
        self.recent.back()
    }
}

/// Вся история партии, которую видит окно.
#[derive(Clone, Debug, Default)]
pub struct History {
    pub counts: Series<Sample>,
    /// Срезы мира (`Snapshot`): геном обоих видов, сытость, где живут и где еда.
    pub snapshots: Series<Snapshot>,
    /// Первый средний геном партии — база «изменения от начала»: в окне
    /// недавнего первая точка уже не начало партии.
    pub origin: Option<[f64; vegetarian::N]>,
    /// Первая сводка генов каждого вида — тоже база «изменения от начала».
    pub vegetarian_origin: Option<[GeneStat; vegetarian::N]>,
    pub predator_origin: Option<[GeneStat; predator::N]>,
}

impl History {
    pub fn add_sample(&mut self, s: Sample) {
        if self.origin.is_none() {
            self.origin = s.genom;
        }
        self.counts.push(s);
    }

    pub fn add_snapshot(&mut self, s: Snapshot) {
        self.vegetarian_origin = self.vegetarian_origin.or(s.genes);
        self.predator_origin = self.predator_origin.or(s.predator_genes);
        self.snapshots.push(s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn недавнее_держит_последние_точки() {
        let mut s = Series::default();
        for i in 0..(RECENT + 50) {
            s.push(i);
        }
        let recent: Vec<usize> = s.points(false).into_iter().copied().collect();
        assert_eq!(recent.len(), RECENT);
        assert_eq!(recent[0], 50);
        assert_eq!(*recent.last().unwrap(), RECENT + 49);
    }

    #[test]
    fn вся_партия_прореживается_и_помнит_начало_и_конец() {
        let mut s = Series::default();
        let n = FULL * 7 + 3;
        for i in 0..n {
            s.push(i);
            let all: Vec<usize> = s.points(true).into_iter().copied().collect();
            assert!(all.len() <= FULL + 1, "память ограничена");
            assert_eq!(all[0], 0, "начало партии не теряется");
            assert_eq!(*all.last().unwrap(), i, "последняя точка есть всегда");
            assert!(all.windows(2).all(|w| w[0] < w[1]), "по возрастанию, без повторов");
        }
        // шаг равномерный: все точки, кроме последней, кратны шагу
        let all: Vec<usize> = s.points(true).into_iter().copied().collect();
        let step = all[1] - all[0];
        assert!(all[..all.len() - 1].iter().all(|x| x % step == 0));
    }

    #[test]
    fn начало_генома_запоминается_с_первого_травоядного() {
        let mut h = History::default();
        let at = |tick, genom| Sample { tick, plants: 0.0, vegetarians: 0.0, predators: 0.0, genom };
        h.add_sample(at(0, None));
        h.add_sample(at(10, Some([1.0; vegetarian::N])));
        h.add_sample(at(20, Some([2.0; vegetarian::N])));
        assert_eq!(h.origin, Some([1.0; vegetarian::N]));
    }
}
