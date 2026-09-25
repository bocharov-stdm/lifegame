//! Временная взаимная неприкосновенность групп после разделения семьи.

use std::collections::{BTreeMap, BTreeSet};

pub const DURATION: u64 = 600;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Grace {
    /// Каноническая пара меток -> последний защищённый тик.
    pairs: BTreeMap<(u64, u64), u64>,
}

impl Grace {
    pub fn register(&mut self, former: u64, new: u64, tick: u64) {
        if former == 0 || new == 0 || former == new {
            return;
        }
        let pair = (former.min(new), former.max(new));
        let until = tick.saturating_add(DURATION);
        self.pairs.entry(pair).and_modify(|old| *old = (*old).max(until)).or_insert(until);
    }

    /// Все группы, возникшие из одной семьи за тик, были своими до разделения.
    /// Защищаем и одновременно отделившиеся ветви друг от друга.
    pub fn register_transitions(&mut self, transitions: &[(u64, u64)], tick: u64) {
        let mut links = BTreeMap::<u64, BTreeSet<u64>>::new();
        for &(former, new) in transitions {
            if former == 0 || new == 0 || former == new {
                continue;
            }
            links.entry(former).or_default().insert(new);
            links.entry(new).or_default().insert(former);
        }
        let mut visited = BTreeSet::new();
        for &start in links.keys() {
            if !visited.insert(start) {
                continue;
            }
            let mut group = vec![start];
            let mut i = 0;
            while i < group.len() {
                let id = group[i];
                for &other in &links[&id] {
                    if visited.insert(other) {
                        group.push(other);
                    }
                }
                i += 1;
            }
            for (i, &a) in group.iter().enumerate() {
                for &b in &group[i + 1..] {
                    self.register(a, b, tick);
                }
            }
        }
    }

    #[inline]
    pub fn contains(&self, a: u64, b: u64, tick: u64) -> bool {
        if a == 0 || b == 0 || a == b {
            return false;
        }
        self.pairs.get(&(a.min(b), a.max(b))).is_some_and(|&until| tick <= until)
    }

    pub fn prune(&mut self, tick: u64) {
        self.pairs.retain(|_, until| tick <= *until);
    }

    pub fn entries(&self) -> impl Iterator<Item = ((u64, u64), u64)> + '_ {
        self.pairs.iter().map(|(&pair, &until)| (pair, until))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn защита_взаимна_и_длится_шестьсот_тиков() {
        let mut grace = Grace::default();
        grace.register(7, 3, 100);
        assert!(grace.contains(3, 7, 101));
        assert!(grace.contains(7, 3, 700));
        assert!(!grace.contains(3, 7, 701));
        grace.prune(701);
        assert_eq!(grace.entries().count(), 0);
    }

    #[test]
    fn повторное_разделение_не_сокращает_срок() {
        let mut grace = Grace::default();
        grace.register(3, 7, 100);
        grace.register(7, 3, 200);
        grace.register(0, 3, 900);
        grace.register(3, 3, 900);
        assert_eq!(grace.entries().collect::<Vec<_>>(), vec![((3, 7), 800)]);
    }

    #[test]
    fn одновременно_отделившиеся_ветви_защищены_друг_от_друга() {
        let mut grace = Grace::default();
        grace.register_transitions(&[(1, 2), (1, 3), (3, 4), (8, 9)], 100);
        for a in 1..=4 {
            for b in a + 1..=4 {
                assert!(grace.contains(a, b, 700), "нет защиты между {a} и {b}");
                assert!(!grace.contains(a, b, 701));
            }
        }
        assert!(!grace.contains(2, 8, 101));
        assert_eq!(grace.entries().count(), 7);
    }
}
