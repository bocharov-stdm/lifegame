//! Species: groups of creatures that could mate — the same diet and genes within the rule
//! `mate_distance` of the group's centre, by the one law of mating (`MateKey`) — followed from sample
//! to sample, and the tree of which split from which; with the measures of novelty (how many program
//! shapes live, how many never seen before, the share of births of two parents).
//!
//! Observation only: taken now and then (`SPECIES_EVERY`), never inside a tick, so the world goes the
//! same with it or without. Its memory is bounded: a group that never grew to `SPECIES_MIN` is
//! forgotten when it dies out, the tree keeps at most `TREE_MAX` species and the shapes seen at
//! most `SHAPES_MAX`. A window takes it as deltas (`Delta`).

use std::collections::{BTreeSet, HashSet};

use life_core::World;
use life_core::creature::{ADULT, Creature};
use life_core::genome::MateKey;
use life_core::genome::creature::N;

/// A group is a species — named in the chronicle, drawn in the tree — once it has had this many
/// creatures at once: a lone mutant past the threshold is no species yet.
pub const SPECIES_MIN: usize = 5;
/// At most this many species in the tree: past it the smallest of the extinct go first.
pub const TREE_MAX: usize = 2000;
/// The species are followed at most once in this many ticks: a species is a slow thing, and a
/// census of everyone's genes against every group is not free on a big world.
pub const SPECIES_EVERY: u64 = 240;
/// At most this many program shapes remembered as seen; past it nothing is new any more (a world
/// would have to run for ages to get there).
const SHAPES_MAX: usize = 200_000;
/// At most this many samples a window keeps: past it every other older one goes (`View::apply`).
const VIEW_POINTS_MAX: usize = 4096;

/// One species: when it arose and from which, how many it has and had at most, its diet, and the
/// centre of its members' genes.
#[derive(Clone, Debug, PartialEq)]
pub struct Species {
    pub id: u32,
    /// The group it split from: the nearest living one when it arose (None: a founders' one). It
    /// may be a passing group: `shown_parent` skips those.
    pub parent: Option<u32>,
    pub born: u64,
    pub died: Option<u64>,
    pub count: usize,
    pub peak: usize,
    /// `Diet` order.
    pub diet: usize,
    centre: MateKey,
}

impl Species {
    /// Grown to a species (`SPECIES_MIN`), not a passing mutant.
    pub fn shown(&self) -> bool {
        self.peak >= SPECIES_MIN
    }
}

/// The measures of one sample.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub tick: u64,
    /// Species alive (`Species::shown`), and groups of any size.
    pub species: usize,
    pub groups: usize,
    /// Distinct adult programs by their shape (`Program::shape`), and how many of them were never
    /// seen before in this world.
    pub shapes: usize,
    pub new_shapes: usize,
    /// The share of births since the last sample that had two parents (`Action::Mate`).
    pub mated_share: f64,
}

/// What changed in a sample, for the chronicle; each with its diet, as the tree may forget it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// A group grew to a species: its id, the shown species it split from (`shown_parent`).
    New {
        id: u32,
        parent: Option<u32>,
        diet: usize,
    },
    Extinct {
        id: u32,
        diet: usize,
    },
}

/// The chronicle's line of a change: «новый вид №12 (травоядный) — отделился от №3». Game UI.
pub fn describe(change: Change) -> (crate::observe::EventKind, String) {
    use crate::observe::EventKind;
    let label = |d: usize| life_core::genome::creature::DIET_VARIANTS[d.min(3)].label;
    match change {
        Change::New { id, parent: Some(p), diet } => {
            (EventKind::SpeciesNew, format!("новый вид №{id} ({}) — отделился от №{p}", label(diet)))
        }
        Change::New { id, parent: None, diet } => {
            (EventKind::SpeciesNew, format!("новый вид №{id} ({})", label(diet)))
        }
        Change::Extinct { id, diet } => {
            (EventKind::SpeciesExtinct, format!("вид №{id} ({}) вымер", label(diet)))
        }
    }
}

/// What changed for a window since it last took the species: the species touched (as they are
/// now), the ones forgotten, and the new samples.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Delta {
    pub species: Vec<Species>,
    pub forgotten: Vec<u32>,
    pub points: Vec<Point>,
}

impl Delta {
    /// A later delta on top of this one: what a window that took neither would take at once.
    pub fn merge(&mut self, later: Delta) {
        for s in later.species {
            upsert(&mut self.species, s);
        }
        self.species.retain(|s| !later.forgotten.contains(&s.id));
        self.forgotten.extend(later.forgotten);
        self.points.extend(later.points);
    }
}

/// `s` in place of the one of its id, or added in the order of ids.
fn upsert(species: &mut Vec<Species>, s: Species) {
    match species.binary_search_by_key(&s.id, |x| x.id) {
        Ok(i) => species[i] = s,
        Err(i) => species.insert(i, s),
    }
}

/// What a window shows of the species, put together from the deltas (`apply`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    pub species: Vec<Species>,
    pub history: Vec<Point>,
}

impl View {
    pub fn apply(&mut self, delta: Delta) {
        for s in delta.species {
            upsert(&mut self.species, s);
        }
        self.species.retain(|s| !delta.forgotten.contains(&s.id));
        self.history.extend(delta.points);
        // a long game: the older half thinned to every other sample, the last one kept
        if self.history.len() > VIEW_POINTS_MAX {
            let half = self.history.len() / 2;
            let mut i = 0;
            self.history.retain(|_| {
                i += 1;
                i > half || i % 2 == 0
            });
        }
    }

    /// The parent of species `id` that is shown, passing groups skipped.
    pub fn shown_parent(&self, id: u32) -> Option<u32> {
        shown_parent(&self.species, id)
    }
}

/// Of `species` (by id), the parent of `id` that is shown (`Species::shown`): the passing groups
/// between are skipped. None for a founders' one.
fn shown_parent(species: &[Species], id: u32) -> Option<u32> {
    let get = |id: u32| species.binary_search_by_key(&id, |s| s.id).ok().map(|i| &species[i]);
    let mut at = get(id)?.parent;
    while let Some(p) = at {
        let s = get(p)?;
        if s.shown() {
            return Some(p);
        }
        at = s.parent;
    }
    None
}

/// The species of a world, followed from sample to sample.
#[derive(Clone, Debug, Default)]
pub struct Tracker {
    species: Vec<Species>,
    next_id: u32,
    shapes_seen: HashSet<u64>,
    born: (u64, u64),
    pub history: Vec<Point>,
    /// For the next delta (`take_delta`): the species touched, those forgotten, and the samples
    /// already given.
    touched: BTreeSet<u32>,
    forgotten: Vec<u32>,
    given: usize,
}

impl Tracker {
    /// Every species it remembers, alive and extinct, by id.
    pub fn species(&self) -> &[Species] {
        &self.species
    }

    /// The last sample's measures.
    pub fn last(&self) -> Option<&Point> {
        self.history.last()
    }

    /// The species shown ever (`Species::shown`), alive or not.
    pub fn shown_ever(&self) -> usize {
        self.species.iter().filter(|s| s.shown()).count()
    }

    /// The parent of species `id` that is shown, passing groups skipped.
    pub fn shown_parent(&self, id: u32) -> Option<u32> {
        shown_parent(&self.species, id)
    }

    /// What changed since the last delta, for a window.
    pub fn take_delta(&mut self) -> Delta {
        let species = std::mem::take(&mut self.touched)
            .into_iter()
            .filter_map(|id| self.species.binary_search_by_key(&id, |s| s.id).ok())
            .map(|i| self.species[i].clone())
            .collect();
        let points = self.history[self.given..].to_vec();
        self.given = self.history.len();
        Delta { species, forgotten: std::mem::take(&mut self.forgotten), points }
    }

    /// A sample: each creature in the order of ID joins the nearest living species of its diet
    /// within the world's `mate_distance` of its centre, else the nearest group arisen in this
    /// sample, else starts a group of its own, split from the nearest living species of its diet
    /// (of any diet for a diet's mutant). Then the centres move to their members' mean and the
    /// empty die out.
    pub fn observe(&mut self, world: &World) -> Vec<Change> {
        let threshold = world.rules.mate_distance;
        let tick = world.tick;
        let living: Vec<usize> =
            (0..self.species.len()).filter(|&i| self.species[i].died.is_none()).collect();
        let centres: Vec<MateKey> = living.iter().map(|&i| self.species[i].centre).collect();
        // (sum of keys, members) of each living species, then of each new group
        let mut sums = vec![([0.0; N], 0_usize); living.len()];
        let mut fresh: Vec<(MateKey, [f64; N], usize, Option<usize>)> = Vec::new();
        let (mut shapes, mut new_shapes) = (HashSet::new(), 0);
        for v in &world.creatures {
            count_shape(&mut shapes, &mut new_shapes, &mut self.shapes_seen, v);
            let key = &v.mate_key;
            let nearest = |centres: &mut dyn Iterator<Item = &MateKey>| {
                centres
                    .enumerate()
                    .map(|(j, c)| (j, key.distance(c)))
                    .filter(|(_, d)| d.is_finite())
                    .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
            };
            match nearest(&mut centres.iter()) {
                Some((j, d)) if d <= threshold => add(&mut sums[j], key),
                old => match nearest(&mut fresh.iter().map(|f| &f.0)) {
                    Some((j, d)) if d <= threshold => {
                        let f = &mut fresh[j];
                        (f.1, f.2) = (std::array::from_fn(|i| f.1[i] + key.coordinates()[i]), f.2 + 1);
                    }
                    _ => {
                        // a diet's mutant split from the nearest species of any diet
                        let parent = old.map(|(j, _)| j).or_else(|| {
                            (0..centres.len()).min_by(|&a, &b| {
                                key.steps(&centres[a]).total_cmp(&key.steps(&centres[b])).then(a.cmp(&b))
                            })
                        });
                        fresh.push((*key, *key.coordinates(), 1, parent));
                    }
                },
            }
        }
        let mut changes = Vec::new();
        for (j, &i) in living.iter().enumerate() {
            let s = &mut self.species[i];
            let (sum, count) = sums[j];
            s.count = count;
            self.touched.insert(s.id);
            if count == 0 {
                s.died = Some(tick);
                if s.shown() {
                    changes.push(Change::Extinct { id: s.id, diet: s.diet });
                }
            } else {
                s.centre = MateKey::centre(&sum, count, s.diet);
                let was = s.shown();
                s.peak = s.peak.max(count);
                if !was && s.shown() {
                    changes.push(Change::New { id: s.id, parent: None, diet: s.diet });
                }
            }
        }
        for (key, sum, count, parent) in fresh {
            let id = self.next_id;
            self.next_id += 1;
            let diet = key.diet();
            let s = Species {
                id,
                parent: parent.map(|j| self.species[living[j]].id),
                born: tick,
                died: None,
                count,
                peak: count,
                diet,
                centre: MateKey::centre(&sum, count, diet),
            };
            if s.shown() && tick > 0 {
                changes.push(Change::New { id, parent: None, diet });
            }
            self.touched.insert(id);
            self.species.push(s);
        }
        // the parent a new species is told of is the one the tree shows, before any is forgotten
        for c in &mut changes {
            if let Change::New { id, parent, .. } = c {
                *parent = shown_parent(&self.species, *id);
            }
        }
        self.forget();
        let c = world.counters;
        // a counter below the last one: another world in its place (a test's); no births counted
        let (born, mated) = (c.born.saturating_sub(self.born.0), c.born_mated.saturating_sub(self.born.1));
        self.born = (c.born, c.born_mated);
        let alive = self.species.iter().filter(|s| s.died.is_none());
        self.history.push(Point {
            tick,
            species: alive.clone().filter(|s| s.shown()).count(),
            groups: alive.count(),
            shapes: shapes.len(),
            new_shapes,
            mated_share: if born > 0 { mated.min(born) as f64 / born as f64 } else { 0.0 },
        });
        changes
    }

    /// Forgets the groups that died out before they grew to a species, and past `TREE_MAX` the
    /// smallest extinct species; their children are moved to their parent, so the tree stays whole.
    fn forget(&mut self) {
        let passing = |s: &Species| s.died.is_some() && !s.shown();
        let mut gone: Vec<(u32, Option<u32>)> =
            self.species.iter().filter(|s| passing(s)).map(|s| (s.id, s.parent)).collect();
        let excess = (self.species.len() - gone.len()).saturating_sub(TREE_MAX);
        if excess > 0 {
            let mut extinct: Vec<&Species> =
                self.species.iter().filter(|s| s.died.is_some() && s.shown()).collect();
            extinct.sort_by_key(|s| (s.peak, s.died.unwrap_or(0) - s.born, s.id));
            gone.extend(extinct.iter().take(excess).map(|s| (s.id, s.parent)));
        }
        if gone.is_empty() {
            return;
        }
        gone.sort_unstable();
        let parent_of = |id: u32| gone.binary_search_by_key(&id, |g| g.0).ok().map(|k| gone[k].1);
        for s in &mut self.species {
            // up past every forgotten one
            let before = s.parent;
            while let Some(up) = s.parent.and_then(parent_of) {
                s.parent = up;
            }
            if s.parent != before {
                self.touched.insert(s.id);
            }
        }
        self.species.retain(|s| parent_of(s.id).is_none());
        for (id, _) in gone {
            self.touched.remove(&id);
            self.forgotten.push(id);
        }
    }
}

/// A key added to a group's sum and count.
fn add(slot: &mut ([f64; N], usize), key: &MateKey) {
    for (x, k) in slot.0.iter_mut().zip(key.coordinates()) {
        *x += k;
    }
    slot.1 += 1;
}

/// Its adult program's shape (`Block::shape`, block by block, nothing allocated) counted among this
/// sample's, and as new if never seen before.
fn count_shape(shapes: &mut HashSet<u64>, new: &mut usize, seen: &mut HashSet<u64>, v: &Creature) {
    let hash = v.programs[ADULT]
        .blocks()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ b.shape()).wrapping_mul(0x100_0000_01b3));
    if shapes.insert(hash) && seen.len() < SHAPES_MAX && seen.insert(hash) {
        *new += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use life_core::genome::creature::Gene;
    use life_core::{CreatureGenome, Rules, WorldConfig};

    /// Two groups of one diet far apart in their genes are two species; the same genes one; another
    /// diet is always another species. A group that empties dies out, and one that never grew to a
    /// species is forgotten. A window put together from the deltas sees what the tracker keeps.
    #[test]
    fn species_are_groups_that_could_mate() {
        let mut w = World::new(&WorldConfig {
            n_creatures: Some(0),
            rules: Rules::default().with("plant_rate", 0.0).unwrap(),
            ..Default::default()
        });
        let far = CreatureGenome::BASE.with(Gene::Size, 40.0 * (0.3_f64 * 9.0 * 4.0).exp());
        let carnivore = CreatureGenome::BASE.with(Gene::Diet, 3.0);
        for (genome, n) in
            [(CreatureGenome::BASE, 6), (far, 6), (carnivore, 6), (far.with(Gene::Speed, 11.0), 1)]
        {
            for _ in 0..n {
                w.spawn(genome, 1000.0, 1000.0, None);
            }
        }
        let mut t = Tracker::default();
        let mut view = View::default();
        let changes = t.observe(&w);
        view.apply(t.take_delta());
        assert!(changes.is_empty(), "the founders' species are no news: {changes:?}");
        assert_eq!(t.species().len(), 3, "{:?}", t.species());
        assert_eq!(t.species().iter().map(|s| s.count).collect::<Vec<_>>(), [6, 7, 6]);
        assert_eq!(t.last().map(|p| (p.species, p.groups, p.shapes)), Some((3, 3, 1)));
        // the carnivores die out; a lone mutant group comes and goes and is forgotten
        w.creatures.retain(|v| v.genome[Gene::Diet] == 0.0);
        let odd = CreatureGenome::BASE.with(Gene::Vision, 400.0 * (0.3_f64 * 9.0 * 5.0).exp());
        w.spawn(odd, 1000.0, 1000.0, None);
        let changes = t.observe(&w);
        // a window that takes no frame for a while: the deltas merge
        let mut pending = t.take_delta();
        assert_eq!(changes, [Change::Extinct { id: 2, diet: 3 }]);
        assert_eq!(describe(changes[0]).1, "вид №2 (мясоед) вымер");
        assert_eq!(t.species().len(), 4, "the lone one is a group of its own");
        assert_eq!(t.species()[3].parent, Some(0), "split from the nearest of its diet");
        w.creatures.pop();
        t.observe(&w);
        pending.merge(t.take_delta());
        view.apply(pending);
        assert_eq!(t.species().len(), 3, "a group that never grew is forgotten");
        assert_eq!(t.shown_ever(), 3);
        assert_eq!(view.species, t.species(), "the deltas add up to what the tracker keeps");
        assert_eq!(view.history, t.history);
    }

    /// A group that grows to a species is told of with the shown species it split from, past a
    /// passing group between; a world in another's place counts no births backwards.
    #[test]
    fn a_new_species_is_told_of_with_its_shown_parent() {
        let mut w = World::new(&WorldConfig {
            n_creatures: Some(0),
            rules: Rules::default().with("plant_rate", 0.0).unwrap(),
            ..Default::default()
        });
        for _ in 0..6 {
            w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, None);
        }
        let mut t = Tracker::default();
        t.observe(&w);
        // a lone mutant, far: a passing group
        let mutant = CreatureGenome::BASE.with(Gene::Size, 40.0 * (0.3_f64 * 9.0 * 4.0).exp());
        w.spawn(mutant, 1000.0, 1000.0, None);
        t.observe(&w);
        // its line, farther still, grows to a species while the mutant lives
        let line = mutant.with(Gene::Speed, 10.0 * (0.3_f64 * 9.0 * 4.0).exp());
        for _ in 0..5 {
            w.spawn(line, 1000.0, 1000.0, None);
        }
        // the founders' species are no news: a later sample's are
        w.tick = SPECIES_EVERY;
        let changes = t.observe(&w);
        assert_eq!(changes, [Change::New { id: 2, parent: Some(0), diet: 0 }], "past the passing #1");
        w.counters.born = 0;
        t.observe(&w);
        assert_eq!(t.last().map(|p| p.mated_share), Some(0.0));
    }
}
