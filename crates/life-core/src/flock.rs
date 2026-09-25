//! Flocks: a family flock is a circle that moves as one object, and its members feed inside it.
//!
//! The circle is also the flock's territory. Its radius comes from the members' inherited
//! spacing (`spacing * sqrt(n)`), its movement from the inherited flock kind. Territoriality
//! decides whether two circles may overlap: two `None` circles overlap freely, a `Moderate` one
//! pushes softly, a `Hard` one is never overlapped. Kin and freshly split groups separate their
//! circles too: grace and kinship only forbid strikes.
//!
//! Everything here runs once per tick over flocks in id order, with a random stream per flock,
//! so the result depends on the seed only.
use crate::{
    Space,
    creature::Creature,
    grid::Grid,
    rng::{Rng, mix},
};
use std::collections::BTreeMap;

/// Circle radius limits: a pair still gets a usable circle, and no circle outgrows a world.
pub const MIN_RADIUS: f64 = 80.0;
pub const MAX_RADIUS: f64 = 600.0;
/// A flock smaller than this is a young family: its circle follows the members and is at
/// least as wide as they see, so a new family forages almost like loners. From this size on
/// the circle is an object that moves by the flock kind.
pub const FAMILY_SIZE: usize = 4;
/// Speed of a circle, as a share of its members' mean speed: members keep up while feeding.
pub const CIRCLE_PACE: f64 = crate::config::SLOW_PACE;
/// The circle waits while fewer than this share of members are inside it.
const WAIT_INSIDE_SHARE: f64 = 0.6;
/// A settled flock moves when its mean fullness stays below this for `HUNGRY_TICKS`...
const HUNGRY_FULLNESS: f64 = 0.4;
const HUNGRY_TICKS: u32 = 600;
/// ...or when no member sees a plant inside the circle for `EMPTY_TICKS`.
const EMPTY_TICKS: u32 = 300;
/// A circle that strict neighbours leave no room shrinks as much as it must; with room it
/// grows back by `RELAX` of its nominal radius per tick. A crowded place thus has smaller
/// territories, and less food per member.
const RELAX: f64 = 0.005;
/// A circle squeezed below this share of its nominal radius...
pub const MIN_COMPRESS: f64 = 0.5;
/// A circle squeezed for this long even at its smallest looks for a free place, and with no
/// room nearby a territorial flock fights for its place instead (`battle.rs`).
const SQUEEZED_TICKS: u32 = 60;
/// Passes of pushing circles apart per tick.
const PASSES: usize = 6;
/// A nomad turns around after this many ticks without progress along its course.
const BLOCKED_TICKS: u32 = 30;
/// Migrants go up for half of the period and down for the other half.
pub const MIGRATION_PERIOD: u64 = 1200;
/// Scouts and migrants pick a new wander target this often.
const WANDER_TICKS: u32 = 600;
/// A soft (moderate) pair removes this share of its overlap per tick, half on each side, and
/// shrinks by `SOFT_SHRINK` of it: moderate circles in a crowd get smaller gradually.
const SOFT_PUSH: f64 = 0.1;
const SOFT_SHRINK: f64 = 0.05;
/// Free place search: rings around the start point, four candidates on each.
const SEARCH_RINGS: usize = 4;
/// A member outside its circle this long leaves the flock with a new label.
pub const STRAY_TICKS: u64 = 300;
/// A strict overlap deeper than this counts as a real overlap (float noise aside).
pub const OVERLAP_TOLERANCE: f64 = 1.0;

/// Inherited way to guard the family flock's circle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Territoriality {
    #[default]
    None,
    Moderate,
    Hard,
}

impl Territoriality {
    pub fn from_gene(value: f64) -> Self {
        if value >= 1.5 {
            Self::Hard
        } else if value >= 0.5 {
            Self::Moderate
        } else {
            Self::None
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "нет",
            Self::Moderate => "умеренная",
            Self::Hard => "жёсткая",
        }
    }
}

/// How the circle of a family flock moves (the `flock_kind` gene).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlockKind {
    /// Stays in place and moves only when food inside runs out.
    #[default]
    Settled,
    /// Drifts slowly along a horizontal course and turns at obstacles.
    Nomadic,
    /// Goes where members saw food.
    Scout,
    /// Goes up and down in depth with `MIGRATION_PERIOD`.
    Migrant,
}

impl FlockKind {
    pub const ALL: [Self; 4] = [Self::Settled, Self::Nomadic, Self::Scout, Self::Migrant];

    pub fn from_gene(value: f64) -> Self {
        Self::ALL.get(value.max(0.0) as usize).copied().unwrap_or_default()
    }

    pub fn label(self) -> &'static str {
        crate::genome::creature::FLOCK_KIND_VARIANTS[self as usize].label
    }
}

/// The circle of a flock: where its members feed and what it guards.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Circle {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
}

impl Circle {
    /// Whether a body of radius `margin` at (x, y) touches the circle.
    #[inline(always)]
    pub fn holds(&self, x: f64, y: f64, margin: f64) -> bool {
        let r = self.radius + margin;
        (x - self.x).powi(2) + (y - self.y).powi(2) <= r * r
    }

    /// The point `share` of the radius from the centre towards (x, y).
    pub fn toward(&self, x: f64, y: f64, share: f64) -> (f64, f64) {
        let (dx, dy) = (x - self.x, y - self.y);
        let d = dx.hypot(dy);
        if d <= 1e-9 {
            return (self.x, self.y);
        }
        let k = (self.radius * share).min(d) / d;
        (self.x + dx * k, self.y + dy * k)
    }
}

/// How two circles treat each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pair {
    /// Two non-territorial circles: overlap freely.
    Free,
    /// A moderate one is involved: push apart gradually.
    Soft,
    /// A hard one is involved: never overlap.
    Strict,
}

pub fn pair(a: Territoriality, b: Territoriality) -> Pair {
    use Territoriality::*;
    if a == Hard || b == Hard {
        Pair::Strict
    } else if a == Moderate || b == Moderate {
        Pair::Soft
    } else {
        Pair::Free
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub id: u64,
    /// Centre of the circle (or the members' mean position when there is none yet).
    pub x: f64,
    pub y: f64,
    /// Radius of the circle: where the flock feeds and what it guards.
    pub radius: f64,
    /// Root mean square distance of the members from their mean position.
    pub spread: f64,
    pub pack_instinct: bool,
    pub territoriality: Territoriality,
    pub kind: FlockKind,
    pub layer_bound: bool,
    pub spacing: f64,
    pub warned: usize,
    pub members: usize,
    /// Members whose body touches the circle.
    pub inside: usize,
    pub juveniles: usize,
    pub sociability: f64,
    pub fullness: f64,
    pub activity: crate::social::Activity,
    /// Where the circle is heading.
    pub goal: Option<(f64, f64)>,
    /// The battle for room the flock fights in.
    pub battle: Option<u64>,
    /// Share of its nominal radius the circle keeps under pressure of neighbours.
    pub compress: f64,
    pub activities: [usize; 5],
}

impl Summary {
    fn add(&mut self, v: &Creature) {
        self.members += 1;
        self.x += v.x;
        self.y += v.y;
        self.juveniles += (!v.adult()) as usize;
        self.sociability += v.pheno.sociability;
        self.fullness += v.energy / v.pheno.max_energy;
        self.spacing += v.pheno.flock_spacing;
        self.activities[v.mind.social.activity as usize] += 1;
        self.pack_instinct = v.pheno.pack_instinct;
        self.territoriality = v.pheno.territoriality;
        self.kind = v.pheno.flock_kind;
        self.layer_bound = v.pheno.layer_bound;
        self.inside += v.circle.is_some_and(|c| c.holds(v.x, v.y, v.pheno.half)) as usize;
    }

    fn finish(&mut self, world: &crate::World) {
        let n = self.members as f64;
        self.x /= n;
        self.y /= n;
        self.sociability /= n;
        self.fullness /= n;
        self.spacing /= n;
        let i = (0..5).max_by_key(|&i| (self.activities[i], std::cmp::Reverse(i))).unwrap();
        self.activity = crate::social::Activity::ALL[i];
        if let Some(f) = world.flocks.get(&self.id) {
            self.warned = f.warned;
            self.battle = f.battle;
            self.compress = f.compress;
            self.goal = f.circle.map(|_| f.target);
            if let Some(c) = f.circle {
                (self.x, self.y, self.radius) = (c.x, c.y, c.radius);
            }
        }
    }
}

pub fn summaries(world: &crate::World) -> Vec<Summary> {
    let mut groups = BTreeMap::<u64, Summary>::new();
    for v in world.creatures.iter().filter(|v| v.alive) {
        groups.entry(v.flock).or_insert_with(|| Summary { id: v.flock, ..Default::default() }).add(v);
    }
    let mut means = BTreeMap::new();
    for s in groups.values() {
        let n = s.members as f64;
        means.insert(s.id, (s.x / n, s.y / n));
    }
    for v in world.creatures.iter().filter(|v| v.alive) {
        let (mx, my) = means[&v.flock];
        groups.get_mut(&v.flock).unwrap().spread += (v.x - mx).powi(2) + (v.y - my).powi(2);
    }
    groups
        .into_values()
        .filter(|s| s.members >= 2)
        .map(|mut s| {
            s.spread = (s.spread / s.members as f64).sqrt();
            s.finish(world);
            s
        })
        .collect()
}

/// Card of one selected flock without building the areas of all other groups.
pub fn summary(world: &crate::World, id: u64) -> Option<Summary> {
    let mut s = Summary { id, ..Default::default() };
    for v in world.creatures.iter().filter(|v| v.alive && v.flock == id) {
        s.add(v);
    }
    if s.members < 2 {
        return None;
    }
    let n = s.members as f64;
    let (mx, my) = (s.x / n, s.y / n);
    for v in world.creatures.iter().filter(|v| v.alive && v.flock == id) {
        s.spread += (v.x - mx).powi(2) + (v.y - my).powi(2);
    }
    s.spread = (s.spread / n).sqrt();
    s.finish(world);
    Some(s)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitWatch {
    pub flock: u64,
    pub anchors: [u64; 3],
    pub since: u64,
}

pub fn split(
    creatures: &mut [Creature],
    grid: &crate::grid::Grid,
    watches: &mut Vec<SplitWatch>,
    next: &mut u64,
    tick: u64,
) -> u64 {
    split_with_transitions(creatures, grid, watches, next, tick, &mut Vec::new())
}

pub fn split_with_transitions(
    creatures: &mut [Creature],
    grid: &crate::grid::Grid,
    watches: &mut Vec<SplitWatch>,
    next: &mut u64,
    tick: u64,
    transitions: &mut Vec<(u64, u64)>,
) -> u64 {
    fn root(parents: &mut [usize], mut i: usize) -> usize {
        for _ in 0..parents.len() {
            if parents[i] == i {
                return i;
            }
            parents[i] = parents[parents[i]];
            i = parents[i];
        }
        unreachable!("дерево компонент без циклов")
    }
    let mut parents: Vec<_> = (0..creatures.len()).collect();
    for (i, v) in creatures.iter().enumerate() {
        grid.for_each_near(v.x, v.y, v.pheno.vision, |j, x, y| {
            if j <= i || creatures[j].flock != v.flock {
                return;
            }
            let d2 = (x - v.x).powi(2) + (y - v.y).powi(2);
            if d2 <= v.pheno.vision2.min(creatures[j].pheno.vision2) {
                let a = root(&mut parents, i);
                let b = root(&mut parents, j);
                parents[a.max(b)] = a.min(b);
            }
        });
    }
    let mut groups = BTreeMap::<(u64, usize), Vec<usize>>::new();
    for (i, v) in creatures.iter().enumerate() {
        groups.entry((v.flock, root(&mut parents, i))).or_default().push(i);
    }
    let mut main = BTreeMap::<u64, (usize, u64, usize)>::new();
    let mut component_counts = BTreeMap::<u64, usize>::new();
    for (&(tag, r), group) in &groups {
        *component_counts.entry(tag).or_default() += 1;
        let min_id = group.iter().map(|&i| creatures[i].id).min().unwrap();
        if main.get(&tag).is_none_or(|&(n, id, _)| group.len() > n || (group.len() == n && min_id < id)) {
            main.insert(tag, (group.len(), min_id, r));
        }
    }
    let mut kept = Vec::new();
    let mut count = 0;
    let mut split_counts = BTreeMap::<u64, usize>::new();
    let mut components: Vec<_> = groups.into_iter().collect();
    components.sort_unstable_by_key(|((tag, _), group)| {
        (*tag, group.iter().map(|&i| creatures[i].id).min().unwrap())
    });
    for ((tag, r), mut group) in components {
        if component_counts[&tag] < 2 || group.len() < 3 {
            continue;
        }
        group.sort_unstable_by_key(|&i| creatures[i].id);
        let ids: std::collections::BTreeSet<_> = group.iter().map(|&i| creatures[i].id).collect();
        let watch = watches
            .iter()
            .filter(|w| w.flock == tag && w.anchors.iter().all(|a| ids.contains(a)))
            .min_by_key(|w| w.since)
            .cloned()
            .unwrap_or(SplitWatch {
                flock: tag,
                anchors: [creatures[group[0]].id, creatures[group[1]].id, creatures[group[2]].id],
                since: tick,
            });
        if main[&tag].2 != r && tick.saturating_sub(watch.since) >= 600 {
            let new_tag = *next;
            *next += 1;
            count += 1;
            *split_counts.entry(tag).or_default() += 1;
            transitions.push((tag, new_tag));
            for i in group {
                creatures[i].flock = new_tag;
                creatures[i].circle = None;
                creatures[i].mind.social = Default::default();
                creatures[i].mind.attack = None;
            }
        } else {
            kept.push(watch);
        }
    }
    kept.retain(|w| component_counts[&w.flock] - split_counts.get(&w.flock).copied().unwrap_or(0) >= 2);
    *watches = kept;
    count
}

/// A member that stayed outside its circle for `STRAY_TICKS` (it could not get back after a
/// chase, a flight or a push) leaves the flock with a new label.
pub fn stragglers(
    creatures: &mut [Creature],
    next: &mut u64,
    tick: u64,
    transitions: &mut Vec<(u64, u64)>,
) -> u64 {
    let mut count = 0;
    for v in creatures.iter_mut().filter(|v| v.alive && v.circle.is_some()) {
        if v.mind.social.outside_since.is_some_and(|t| tick.saturating_sub(t) >= STRAY_TICKS) {
            transitions.push((v.flock, *next));
            v.flock = *next;
            *next += 1;
            v.circle = None;
            v.mind.social = Default::default();
            v.mind.attack = None;
            count += 1;
        }
    }
    count
}

#[derive(Clone, Debug)]
pub struct Flock {
    /// None until the flock has two members.
    pub circle: Option<Circle>,
    /// Where the circle is heading now.
    pub target: (f64, f64),
    pub pack_instinct: bool,
    pub territoriality: Territoriality,
    pub kind: FlockKind,
    pub layer_bound: bool,
    pub warned: usize,
    pub alarmed: bool,
    /// Scouts: a member's fresh food report is the target.
    pub food_goal: bool,
    pub food_since: u64,
    pub last_food: Option<u64>,
    pub food_target: (f64, f64),
    /// The latest fresh food report, a preferred place when the flock moves.
    pub food_hint: Option<(f64, f64)>,
    pub members: usize,
    /// Members whose body touches the circle.
    pub inside: usize,
    /// Mean member position.
    pub center: (f64, f64),
    /// A move to a new place (no food, no room); overrides the kind's own target.
    pub moving_to: Option<(f64, f64)>,
    /// Nomads: -1 or 1 along x.
    pub heading: f64,
    /// Ticks since the flock got its circle; the migrant phase counts from it.
    pub age: u64,
    /// Ticks until a new wander target.
    pub remaining: u32,
    pub hungry: u32,
    pub empty: u32,
    pub squeezed: u32,
    pub blocked: u32,
    /// Share of the nominal radius the circle has under pressure of strict neighbours.
    pub compress: f64,
    /// Squeezed with no room nearby and ready to fight for the place (combat on).
    pub cornered: bool,
    /// Moves made because of squeezing into a place without room; a moderate flock fights
    /// only after such a move. Reset when the circle has its full radius again.
    pub cornered_moves: u32,
    /// The battle the flock takes part in.
    pub battle: Option<u64>,
    /// Ticks until the flock may fight again after a battle.
    pub calm: u32,
    pub rng: Rng,
    /// Mean layer of the members (the whole depth for free ones), speed, vision, fullness.
    pub layer: (f64, f64),
    pub speed: f64,
    pub vision: f64,
    pub fullness: f64,
    pub spacing: f64,
    /// Members that see a plant inside the circle this tick.
    fed: usize,
}

impl Flock {
    fn new(seed: u64, tag: u64) -> Self {
        Flock {
            circle: None,
            target: (0.0, 0.0),
            pack_instinct: false,
            territoriality: Territoriality::None,
            kind: FlockKind::Settled,
            layer_bound: true,
            warned: 0,
            alarmed: false,
            food_goal: false,
            food_since: 0,
            last_food: None,
            food_target: (0.0, 0.0),
            food_hint: None,
            members: 0,
            inside: 0,
            center: (0.0, 0.0),
            moving_to: None,
            heading: 0.0,
            age: 0,
            remaining: 0,
            hungry: 0,
            empty: 0,
            squeezed: 0,
            blocked: 0,
            compress: 1.0,
            cornered: false,
            cornered_moves: 0,
            battle: None,
            calm: 0,
            rng: Rng::keyed(seed, tag),
            layer: (0.0, 0.0),
            speed: 0.0,
            vision: 0.0,
            fullness: 0.0,
            spacing: 0.0,
            fed: 0,
        }
    }

    /// Radius of the circle for the current members, without pressure of neighbours.
    fn nominal(&self) -> f64 {
        let r = self.spacing * (self.members as f64).sqrt();
        let r = if self.young() { r.max(self.vision) } else { r };
        r.clamp(MIN_RADIUS, MAX_RADIUS)
    }

    /// A young family: its circle follows the members.
    pub fn young(&self) -> bool {
        self.members < FAMILY_SIZE
    }

    /// Radius of the circle for the current members and the pressure of neighbours.
    fn radius(&self) -> f64 {
        self.nominal() * self.compress
    }

    /// Squeezed to its smallest circle for a while.
    fn is_squeezed(&self) -> bool {
        self.squeezed >= SQUEEZED_TICKS && self.compress <= MIN_COMPRESS
    }

    /// Where the centre of a circle of radius `r` may stand: inside the world, and for a flock
    /// bound to its layer, inside the members' mean layer.
    fn bounds(&self, r: f64, space: &Space) -> Bounds {
        let (x0, x1) = fit(r, space.width);
        let (wy0, wy1) = fit(r, space.height);
        let (lo, hi) = self.layer;
        let (y0, y1) = (lo.clamp(wy0, wy1), hi.clamp(wy0, wy1));
        Bounds { x0, x1, y0: y0.min(y1), y1: y0.max(y1) }
    }
}

/// Allowed centre positions of one circle.
#[derive(Clone, Copy, Debug)]
struct Bounds {
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
}

impl Bounds {
    fn clamp(&self, (x, y): (f64, f64)) -> (f64, f64) {
        (x.clamp(self.x0, self.x1), y.clamp(self.y0, self.y1))
    }
}

/// Centre range for a circle of radius `r` along an axis of length `len`.
fn fit(r: f64, len: f64) -> (f64, f64) {
    if 2.0 * r >= len { (len / 2.0, len / 2.0) } else { (r, len - r) }
}

/// Circles of all flocks, with a grid over their centres, for overlap queries.
struct Circles {
    items: Vec<Item>,
    grid: Grid,
    max_r: f64,
}

#[derive(Clone, Copy, Debug)]
struct Item {
    id: u64,
    x: f64,
    y: f64,
    r: f64,
    mode: Territoriality,
    /// Where the centre may stand (the flock's layer inside the world).
    bounds: Bounds,
}

impl Circles {
    fn of(flocks: &BTreeMap<u64, Flock>, space: &Space) -> Self {
        let items: Vec<_> = flocks
            .iter()
            .filter_map(|(&id, f)| {
                let c = f.circle?;
                Some(Item {
                    id,
                    x: c.x,
                    y: c.y,
                    r: c.radius,
                    mode: f.territoriality,
                    bounds: f.bounds(c.radius, space),
                })
            })
            .collect();
        let max_r = items.iter().fold(MIN_RADIUS, |m, c| m.max(c.r));
        let mut grid = Grid::new((2.0 * max_r).max(crate::config::GRID_CELL));
        grid.rebuild(space, items.iter().map(|c| (c.x, c.y)));
        Circles { items, grid, max_r }
    }

    /// Overlap of a circle at (x, y) with every neighbour it must not overlap.
    fn conflict(&self, skip: u64, x: f64, y: f64, r: f64, mode: Territoriality) -> f64 {
        let mut total = 0.0;
        self.grid.for_each_near(x, y, r + self.max_r, |j, _, _| {
            let c = &self.items[j];
            if c.id == skip || pair(mode, c.mode) == Pair::Free {
                return;
            }
            total += (r + c.r - (c.x - x).hypot(c.y - y)).max(0.0);
        });
        total
    }

    /// A place for a circle near `from`: `prefer` or `from` itself when free, else the first
    /// free point on rings around `from`, else the least crowded one. `away`: (x, y, d) — a
    /// place to leave; candidates closer than `d` to it are skipped.
    #[allow(clippy::too_many_arguments)]
    fn free_spot(
        &self,
        skip: u64,
        from: (f64, f64),
        prefer: Option<(f64, f64)>,
        away: Option<(f64, f64, f64)>,
        r: f64,
        mode: Territoriality,
        bounds: Bounds,
        rng: &mut Rng,
    ) -> (f64, f64) {
        let turn = rng.uniform(0.0, std::f64::consts::TAU);
        let rings = (1..=SEARCH_RINGS).flat_map(|k| {
            (0..4).map(move |m| {
                let angle = turn
                    + m as f64 * std::f64::consts::FRAC_PI_2
                    + (k % 2) as f64 * 0.25 * std::f64::consts::PI;
                let d = 2.0 * r * k as f64;
                (from.0 + angle.cos() * d, from.1 + angle.sin() * d)
            })
        });
        let candidates: Vec<_> =
            prefer.into_iter().chain([from]).chain(rings).map(|p| bounds.clamp(p)).collect();
        let allowed = |p: &(f64, f64)| away.is_none_or(|(x, y, d)| (p.0 - x).hypot(p.1 - y) >= d);
        // With nothing allowed (a corner of the world), the farthest candidate from `away`.
        let mut best = (f64::INFINITY, *candidates.last().unwrap());
        if let Some((x, y, _)) = away {
            best.1 = *candidates
                .iter()
                .max_by(|a, b| (a.0 - x).hypot(a.1 - y).total_cmp(&(b.0 - x).hypot(b.1 - y)))
                .unwrap();
        }
        for p in candidates.into_iter().filter(allowed) {
            let overlap = self.conflict(skip, p.0, p.1, r, mode);
            if overlap <= OVERLAP_TOLERANCE {
                return p;
            }
            if overlap < best.0 {
                best = (overlap, p);
            }
        }
        best.1
    }

    /// Visit every overlapping pair that is not free: (i, j, overlap, pair, unit i -> j).
    fn overlaps(&self, mut f: impl FnMut(usize, usize, f64, Pair, (f64, f64))) {
        for i in 0..self.items.len() {
            let a = self.items[i];
            self.grid.for_each_near(a.x, a.y, a.r + self.max_r, |j, _, _| {
                if j <= i {
                    return;
                }
                let b = self.items[j];
                let rule = pair(a.mode, b.mode);
                if rule == Pair::Free {
                    return;
                }
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let d = dx.hypot(dy);
                let overlap = a.r + b.r - d;
                if overlap <= 0.0 {
                    return;
                }
                f(i, j, overlap, rule, apart(dx, dy, d));
            });
        }
    }
}

/// Unit vector from one circle's centre to another's; circles on one centre part along x (a
/// layer may pin their depth, never their width), the later one eastwards.
fn apart(dx: f64, dy: f64, d: f64) -> (f64, f64) {
    if d > 1e-9 { (dx / d, dy / d) } else { (1.0, 0.0) }
}

/// Overlap between circles that must not overlap: pairs involving a hard circle (strict), and
/// pairs involving a moderate one (soft, allowed to linger while they part).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OverlapStats {
    pub strict_pairs: usize,
    pub strict_depth: f64,
    pub soft_pairs: usize,
    pub soft_depth: f64,
}

pub fn overlap_stats(flocks: &BTreeMap<u64, Flock>, space: &Space) -> OverlapStats {
    let circles = Circles::of(flocks, space);
    let mut s = OverlapStats::default();
    circles.overlaps(|_, _, overlap, rule, _| {
        if overlap <= OVERLAP_TOLERANCE {
            return;
        }
        if rule == Pair::Strict {
            s.strict_pairs += 1;
            s.strict_depth += overlap;
        } else {
            s.soft_pairs += 1;
            s.soft_depth += overlap;
        }
    });
    s
}

/// Recount flock membership, give new flocks a circle and, with `advance`, move the circles
/// one tick and push apart the ones that must not overlap. Every creature gets its flock's
/// circle. Returns how many flocks started a move to a new place.
#[doc(hidden)]
pub fn update(
    flocks: &mut BTreeMap<u64, Flock>,
    creatures: &mut [Creature],
    space: &Space,
    seed: u64,
    advance: bool,
) -> u64 {
    update_full(flocks, creatures, space, seed, advance, false)
}

/// `update`, where with `combat` a cornered territorial flock gets ready to fight instead of
/// moving (`Flock::cornered`).
#[doc(hidden)]
pub fn update_full(
    flocks: &mut BTreeMap<u64, Flock>,
    creatures: &mut [Creature],
    space: &Space,
    seed: u64,
    advance: bool,
    combat: bool,
) -> u64 {
    for f in flocks.values_mut() {
        f.members = 0;
        f.inside = 0;
        f.fed = 0;
        f.center = (0.0, 0.0);
        f.layer = (0.0, 0.0);
        (f.speed, f.vision, f.fullness, f.spacing) = (0.0, 0.0, 0.0, 0.0);
    }
    for v in creatures.iter().filter(|v| v.alive) {
        let f = flocks.entry(v.flock).or_insert_with(|| Flock::new(seed, v.flock));
        f.members += 1;
        f.pack_instinct = v.pheno.pack_instinct;
        f.territoriality = v.pheno.territoriality;
        f.kind = v.pheno.flock_kind;
        f.layer_bound = v.pheno.layer_bound;
        f.center.0 += v.x;
        f.center.1 += v.y;
        f.layer.0 += v.pheno.layer_lo;
        f.layer.1 += v.pheno.layer_hi;
        f.speed += v.pheno.speed;
        f.vision += v.pheno.vision;
        f.fullness += v.energy / v.pheno.max_energy;
        f.spacing += v.pheno.flock_spacing;
        // a plant a forager sees outside the circle does not keep a settled flock in place
        f.fed += v
            .mind
            .social
            .personal_food
            .is_some_and(|(x, y)| f.circle.is_some_and(|c| c.holds(x, y, v.pheno.half)))
            as usize;
        f.inside += f.circle.is_some_and(|c| c.holds(v.x, v.y, v.pheno.half)) as usize;
    }
    flocks.retain(|_, f| f.members > 0);
    for f in flocks.values_mut() {
        let n = f.members as f64;
        f.center = (f.center.0 / n, f.center.1 / n);
        f.layer = (f.layer.0 / n, f.layer.1 / n);
        (f.speed, f.vision, f.fullness, f.spacing) =
            (f.speed / n, f.vision / n, f.fullness / n, f.spacing / n);
        if f.members < 2 {
            f.circle = None;
            continue;
        }
        let r = f.radius();
        let bounds = f.bounds(r, space);
        if let Some(c) = &mut f.circle {
            c.radius = r;
            (c.x, c.y) = bounds.clamp((c.x, c.y));
        }
    }
    place_new(flocks, space);
    let mut moves = 0;
    if advance {
        moves += start_moves(flocks, space, combat);
        move_circles(flocks, space);
        resolve(flocks, space);
    } else {
        // Births and the layer clamp above may have grown a circle into a strict neighbour.
        let mut circles = Circles::of(flocks, space);
        shrink_strict(&mut circles);
        for item in &circles.items {
            let f = flocks.get_mut(&item.id).unwrap();
            let nominal = f.nominal();
            f.circle.as_mut().unwrap().radius = item.r;
            f.compress = item.r / nominal;
        }
    }
    for v in creatures {
        v.circle = flocks.get(&v.flock).and_then(|f| f.circle);
    }
    moves
}

/// A new flock gets its circle where its members are, or at the nearest free place.
fn place_new(flocks: &mut BTreeMap<u64, Flock>, space: &Space) {
    let fresh: Vec<u64> =
        flocks.iter().filter(|(_, f)| f.members >= 2 && f.circle.is_none()).map(|(&id, _)| id).collect();
    if fresh.is_empty() {
        return;
    }
    let mut circles = Circles::of(flocks, space);
    for id in fresh {
        let f = flocks.get_mut(&id).unwrap();
        let r = f.radius();
        let bounds = f.bounds(r, space);
        let spot = circles.free_spot(id, f.center, None, None, r, f.territoriality, bounds, &mut f.rng);
        f.circle = Some(Circle { x: spot.0, y: spot.1, radius: r });
        f.target = spot;
        f.age = 0;
        f.remaining = 0;
        f.heading = if f.rng.random() < 0.5 { -1.0 } else { 1.0 };
        // later placements see this circle
        circles = Circles::of(flocks, space);
    }
}

/// Flocks that must move: settled ones out of food, any flock squeezed by strict neighbours.
/// With `combat`, a squeezed territorial flock with no room nearby stays and gets ready to
/// fight (a hard one at once, a moderate one after one such move did not help).
fn start_moves(flocks: &mut BTreeMap<u64, Flock>, space: &Space, combat: bool) -> u64 {
    let mut wanted = Vec::new();
    for (&id, f) in flocks.iter_mut().filter(|(_, f)| f.circle.is_some()) {
        f.hungry = if f.fullness < HUNGRY_FULLNESS { f.hungry + 1 } else { 0 };
        f.empty = if f.fed == 0 { f.empty + 1 } else { 0 };
        f.calm = f.calm.saturating_sub(1);
        f.cornered = false;
        if f.compress >= 1.0 {
            f.cornered_moves = 0;
        }
        if f.battle.is_some() || f.young() {
            continue; // it fights for this place instead of leaving it; a family roams anyway
        }
        let starving = f.kind == FlockKind::Settled && (f.hungry >= HUNGRY_TICKS || f.empty >= EMPTY_TICKS);
        if f.moving_to.is_none() && (starving || f.is_squeezed()) {
            wanted.push(id);
        }
    }
    if wanted.is_empty() {
        return 0;
    }
    let circles = Circles::of(flocks, space);
    let mut moves = 0;
    for &id in &wanted {
        let f = flocks.get_mut(&id).unwrap();
        let c = f.circle.unwrap();
        // A squeezed flock looks around itself for room for its full circle; a hungry one
        // prefers reported food, away from the eaten-out place.
        let squeezed = f.is_squeezed();
        let r = if squeezed { f.nominal() } else { c.radius };
        let bounds = f.bounds(r, space);
        let prefer = f.food_hint.filter(|&(x, y)| (x - c.x).hypot(y - c.y) > c.radius);
        let from = if squeezed { (c.x, c.y) } else { prefer.unwrap_or((c.x, c.y)) };
        let away = (!squeezed).then_some((c.x, c.y, 1.5 * c.radius));
        let spot = circles.free_spot(id, from, prefer, away, r, f.territoriality, bounds, &mut f.rng);
        if squeezed && circles.conflict(id, spot.0, spot.1, r, f.territoriality) > OVERLAP_TOLERANCE {
            let fights = combat
                && f.calm == 0
                && match f.territoriality {
                    Territoriality::Hard => true,
                    Territoriality::Moderate => f.cornered_moves >= 1,
                    Territoriality::None => false,
                };
            if fights {
                f.cornered = true;
                continue;
            }
            f.cornered_moves += 1;
        }
        f.moving_to = Some(spot);
        (f.hungry, f.empty, f.squeezed) = (0, 0, 0);
        moves += 1;
    }
    moves
}

/// Flocks beaten in a battle move away from it, to a free place if there is one nearby.
pub(crate) fn retreat(flocks: &mut BTreeMap<u64, Flock>, space: &Space, ids: &[u64]) {
    let circles = Circles::of(flocks, space);
    for &id in ids {
        let Some(f) = flocks.get_mut(&id) else { continue };
        let Some(c) = f.circle else { continue };
        let r = f.nominal();
        let bounds = f.bounds(r, space);
        let away = Some((c.x, c.y, c.radius + r));
        let spot =
            circles.free_spot(id, (c.x, c.y), f.food_hint, away, r, f.territoriality, bounds, &mut f.rng);
        f.moving_to = Some(spot);
        (f.hungry, f.empty, f.squeezed) = (0, 0, 0);
    }
}

fn move_circles(flocks: &mut BTreeMap<u64, Flock>, space: &Space) {
    for (&id, f) in flocks.iter_mut() {
        let Some(mut c) = f.circle else { continue };
        f.age += 1;
        f.remaining = f.remaining.saturating_sub(1);
        let bounds = f.bounds(c.radius, space);
        if f.young() {
            // A young family roams: the circle goes after its members, no faster than they walk.
            // A family beaten in a battle moves away first (`retreat`), then roams again.
            f.target = bounds.clamp(f.moving_to.unwrap_or(f.center));
            let (dx, dy) = (f.target.0 - c.x, f.target.1 - c.y);
            let d = dx.hypot(dy);
            let k = if d <= f.speed { 1.0 } else { f.speed / d };
            (c.x, c.y) = bounds.clamp((c.x + dx * k, c.y + dy * k));
            if d <= f.speed && f.moving_to.take().is_some() {
                (f.hungry, f.empty) = (0, 0);
            }
            f.circle = Some(c);
            continue;
        }
        let speed = CIRCLE_PACE * f.speed;
        let target = match f.moving_to {
            Some(t) => t,
            None => kind_target(id, f, c, bounds, speed, space),
        };
        f.target = bounds.clamp(target);
        if (f.inside as f64) < WAIT_INSIDE_SHARE * f.members as f64 {
            continue; // members are not with the circle yet
        }
        let (dx, dy) = (f.target.0 - c.x, f.target.1 - c.y);
        let d = dx.hypot(dy);
        if d <= speed {
            (c.x, c.y) = f.target;
            if f.moving_to.take().is_some() {
                (f.hungry, f.empty) = (0, 0);
            }
        } else {
            c.x += dx / d * speed;
            c.y += dy / d * speed;
        }
        (c.x, c.y) = bounds.clamp((c.x, c.y));
        f.circle = Some(c);
    }
}

/// Target of the circle by its flock kind.
fn kind_target(id: u64, f: &mut Flock, c: Circle, bounds: Bounds, speed: f64, space: &Space) -> (f64, f64) {
    match f.kind {
        FlockKind::Settled => (c.x, c.y),
        FlockKind::Nomadic => {
            if f.heading == 0.0 {
                f.heading = 1.0;
            }
            let at_edge = (f.heading < 0.0 && c.x <= bounds.x0 + speed)
                || (f.heading > 0.0 && c.x >= bounds.x1 - speed);
            if at_edge || f.blocked >= BLOCKED_TICKS {
                f.heading = -f.heading;
                f.blocked = 0;
            }
            if f.remaining == 0 {
                // a slow wander in depth inside the layer
                f.target.1 = f.rng.uniform(c.y - c.radius, c.y + c.radius).clamp(bounds.y0, bounds.y1);
                f.remaining = WANDER_TICKS;
            }
            (c.x + f.heading * 4.0 * c.radius.max(speed), f.target.1)
        }
        FlockKind::Scout => {
            if f.food_goal {
                return f.food_target;
            }
            let arrived = (f.target.0 - c.x).hypot(f.target.1 - c.y) <= speed;
            if f.remaining == 0 || arrived {
                let angle = f.rng.uniform(0.0, std::f64::consts::TAU);
                let d = f.rng.uniform(0.5, 3.0) * f.vision;
                f.target = bounds.clamp((c.x + angle.cos() * d, c.y + angle.sin() * d));
                f.remaining = WANDER_TICKS;
            }
            f.target
        }
        FlockKind::Migrant => {
            let (top, bottom) = if f.layer_bound {
                (bounds.y0, bounds.y1)
            } else {
                let (lo, hi) = fit(c.radius, space.height);
                ((0.05 * space.height).clamp(lo, hi), (0.5 * space.height).clamp(lo, hi))
            };
            let up = (f.age + mix(id)) % MIGRATION_PERIOD < MIGRATION_PERIOD / 2;
            if f.remaining == 0 {
                f.target.0 = (c.x + f.rng.uniform(-f.vision, f.vision)).clamp(bounds.x0, bounds.x1);
                f.remaining = WANDER_TICKS;
            }
            (f.target.0, if up { top } else { bottom })
        }
    }
}

/// Share of a strict overlap each side of the pair takes: a hard circle yields nothing to a
/// softer one, two hard ones split it.
fn strict_shares(a: Territoriality, b: Territoriality) -> (f64, f64) {
    match (a == Territoriality::Hard, b == Territoriality::Hard) {
        (true, false) => (0.0, 1.0),
        (false, true) => (1.0, 0.0),
        _ => (0.5, 0.5),
    }
}

/// Grow circles with room, push apart the ones that must not overlap, in id order, and shrink
/// strict pairs out of whatever overlap the pushes left (a wall of the world or of the layer,
/// a crowd). Shrinking never creates a new overlap, so after it no strict pair overlaps.
fn resolve(flocks: &mut BTreeMap<u64, Flock>, space: &Space) {
    for f in flocks.values_mut() {
        let nominal = f.nominal();
        if let Some(c) = &mut f.circle {
            f.compress = (f.compress + RELAX).min(1.0);
            c.radius = nominal * f.compress;
        }
    }
    let mut circles = Circles::of(flocks, space);
    let grown: Vec<f64> = circles.items.iter().map(|c| c.r).collect();
    if circles.items.len() >= 2 {
        for _ in 0..PASSES {
            let mut pushes = Vec::new();
            circles.overlaps(|i, j, overlap, rule, unit| pushes.push((i, j, overlap, rule, unit)));
            if pushes.is_empty() {
                break;
            }
            for (i, j, _, rule, _) in pushes {
                // positions changed by earlier pushes of this pass: measure again
                let (a, b) = (circles.items[i], circles.items[j]);
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let d = dx.hypot(dy);
                let overlap = a.r + b.r - d;
                if overlap <= 0.0 {
                    continue;
                }
                let (ux, uy) = apart(dx, dy, d);
                let (sa, sb) = match rule {
                    Pair::Strict => strict_shares(a.mode, b.mode),
                    Pair::Soft => (SOFT_PUSH / 2.0, SOFT_PUSH / 2.0),
                    Pair::Free => (0.0, 0.0),
                };
                let (ax, ay) = a.bounds.clamp((a.x - ux * overlap * sa, a.y - uy * overlap * sa));
                let (bx, by) = b.bounds.clamp((b.x + ux * overlap * sb, b.y + uy * overlap * sb));
                (circles.items[i].x, circles.items[i].y) = (ax, ay);
                (circles.items[j].x, circles.items[j].y) = (bx, by);
            }
            circles.grid.rebuild(space, circles.items.iter().map(|c| (c.x, c.y)));
        }
    }
    let mut soft = Vec::new();
    circles.overlaps(|i, j, overlap, rule, _| {
        if rule == Pair::Soft {
            soft.push((i, j, overlap));
        }
    });
    for (i, j, overlap) in soft {
        for k in [i, j] {
            circles.items[k].r = (circles.items[k].r - overlap * SOFT_SHRINK / 2.0).max(1.0);
        }
    }
    shrink_strict(&mut circles);
    for (item, grown) in circles.items.iter().zip(grown) {
        let f = flocks.get_mut(&item.id).unwrap();
        let nominal = f.nominal();
        let c = f.circle.as_mut().unwrap();
        let progress = (item.x - c.x) * f.heading;
        let squeezed = item.r < grown - 1e-9;
        (c.x, c.y, c.radius) = (item.x, item.y, item.r);
        f.squeezed = if squeezed { f.squeezed + 1 } else { 0 };
        f.compress = item.r / nominal;
        // A nomad pushed back against its course does not progress.
        if f.kind == FlockKind::Nomadic {
            f.blocked = if progress < 0.0 { f.blocked + 1 } else { 0 };
        }
    }
}

/// Shrink every strict pair out of its overlap. Shrinking never creates a new overlap, so one
/// pass in id order leaves none.
fn shrink_strict(circles: &mut Circles) {
    let mut strict = Vec::new();
    circles.overlaps(|i, j, _, rule, _| {
        if rule == Pair::Strict {
            strict.push((i, j));
        }
    });
    for (i, j) in strict {
        let (a, b) = (circles.items[i], circles.items[j]);
        let overlap = a.r + b.r - (b.x - a.x).hypot(b.y - a.y);
        if overlap <= 0.0 {
            continue;
        }
        let (sa, _) = strict_shares(a.mode, b.mode);
        // what one side cannot give, the other gives
        let da = (overlap * sa).min(a.r);
        let db = (overlap - da).min(b.r);
        let da = (overlap - db).min(a.r);
        circles.items[i].r -= da;
        circles.items[j].r -= db;
    }
}

/// An overcrowded flock loses one adult per tick with a growing chance.
pub fn departures(creatures: &mut [Creature], next: &mut u64, tick: u64) -> u64 {
    departures_with_transitions(creatures, next, tick, &mut Vec::new())
}

pub fn departures_with_transitions(
    creatures: &mut [Creature],
    next: &mut u64,
    tick: u64,
    transitions: &mut Vec<(u64, u64)>,
) -> u64 {
    let mut groups = BTreeMap::<u64, (usize, f64, f64)>::new();
    for v in creatures.iter() {
        let g = groups.entry(v.flock).or_default();
        g.0 += 1;
        g.1 += v.x;
        g.2 += v.y;
    }
    let mut chosen = BTreeMap::<u64, (usize, f64, u64)>::new();
    for (i, v) in creatures.iter().enumerate() {
        let Some(&(n, sx, sy)) = groups.get(&v.flock).filter(|&&(n, _, _)| n > 50) else { continue };
        if !v.adult() {
            continue;
        }
        let d2 = (v.x - sx / n as f64).powi(2) + (v.y - sy / n as f64).powi(2);
        if chosen.get(&v.flock).is_none_or(|&(_, old, id)| d2 > old || (d2 == old && v.id < id)) {
            chosen.insert(v.flock, (i, d2, v.id));
        }
    }
    let mut count = 0;
    for (tag, (i, _, _)) in chosen {
        let extra = (groups[&tag].0 - 50) as f64;
        let probability = (0.06 * extra * extra).min(1.0);
        let roll = (crate::rng::mix(tag ^ crate::rng::mix(tick)) >> 11) as f64 / (1u64 << 53) as f64;
        if roll < probability {
            creatures[i].flock = *next;
            transitions.push((tag, *next));
            *next += 1;
            creatures[i].circle = None;
            creatures[i].mind.social = Default::default();
            creatures[i].mind.attack = None;
            count += 1;
        }
    }
    count
}

/// Food reports of members, every 60 ticks: the best one becomes a scout flock's target and
/// every flock's hint for its next move.
pub(crate) fn food_goals(flocks: &mut BTreeMap<u64, Flock>, creatures: &[Creature], tick: u64) {
    if !tick.is_multiple_of(60) {
        return;
    }
    let mut best = BTreeMap::<u64, (f64, crate::social::Food)>::new();
    for v in creatures.iter() {
        let Some(food) = v.mind.social.observed_food.filter(|f| f.fresh(tick)) else { continue };
        let Some(c) = flocks.get(&v.flock).and_then(|f| f.circle) else { continue };
        let travel = (food.x - c.x).hypot(food.y - c.y) / v.pheno.speed.max(0.01);
        let score = v.pheno.plant_energy * v.pheno.plant_efficiency / (travel + 1.0);
        if best
            .get(&v.flock)
            .is_none_or(|(old, prior)| score > *old || (score == *old && food.observer < prior.observer))
        {
            best.insert(v.flock, (score, food));
        }
    }
    for (tag, f) in flocks.iter_mut() {
        if let Some((_, food)) = best.get(tag) {
            f.last_food = Some(food.tick);
            f.food_hint = Some((food.x, food.y));
            if f.kind == FlockKind::Scout && (!f.food_goal || tick.saturating_sub(f.food_since) >= 180) {
                f.food_target = (food.x, food.y);
                f.food_goal = true;
                f.food_since = tick;
            }
        } else if f.last_food.is_none_or(|t| tick.saturating_sub(t) >= 180) {
            f.food_hint = None;
            if f.food_goal {
                f.food_goal = false;
                f.remaining = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::creature::Gene;
    use crate::{CreatureGenome, World, WorldConfig};

    fn world() -> World {
        World::new(&WorldConfig {
            n_creatures: Some(0),
            rules: crate::Rules::default().with("plant_rate", 0.0).unwrap(),
            ..Default::default()
        })
    }

    /// A flock of `n` with the given genome around (x, y); returns its tag.
    fn flock_at(w: &mut World, genome: CreatureGenome, n: usize, x: f64, y: f64) -> u64 {
        let first = w.creatures.len();
        for i in 0..n {
            let angle = i as f64;
            w.spawn(genome, x + 20.0 * angle.cos(), y + 20.0 * angle.sin(), Some(100.0));
        }
        let tag = w.creatures[first].flock;
        for v in &mut w.creatures[first..] {
            v.flock = tag;
            v.reproduction_wait = 1_000_000;
        }
        tag
    }

    #[test]
    fn выбранная_стая_совпадает_с_общим_срезом_и_исчезает_без_пары() {
        let mut world = World::new(&WorldConfig { n_creatures: Some(3), ..Default::default() });
        let id = world.creatures[0].flock;
        world.creatures[1].flock = id;
        let all = summaries(&world);
        assert_eq!(all.len(), 1);
        assert_eq!(summary(&world, id), Some(all[0].clone()));
        world.creatures[1].alive = false;
        assert_eq!(summary(&world, id), None);
    }

    #[test]
    fn радиус_круга_растёт_как_корень_из_числа_участников_в_пределах() {
        for (spacing, n, want) in
            [(60.0, 4, 120.0_f64), (60.0, 16, 240.0), (10.0, 4, 2.0 * 50.0), (500.0, 50, 700.0)]
        {
            let mut w = world();
            let tag =
                flock_at(&mut w, CreatureGenome::BASE.with(Gene::FlockSpacing, spacing), n, 3000.0, 1500.0);
            update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
            let r = w.flocks[&tag].circle.unwrap().radius;
            assert!((r - want.clamp(MIN_RADIUS, MAX_RADIUS)).abs() < 1e-9, "spacing {spacing}, n {n}: {r}");
            assert!(w.creatures.iter().all(|v| v.circle == w.flocks[&tag].circle));
        }
    }

    #[test]
    fn одиночка_не_получает_круга() {
        let mut w = world();
        w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, None);
        update(&mut w.flocks, &mut w.creatures, &w.space, 1, true);
        assert!(w.creatures[0].circle.is_none());
    }

    #[test]
    fn жёсткий_круг_не_перекрывается_в_тесноте() {
        let mut w = world();
        let hard = CreatureGenome::BASE.with(Gene::Territoriality, 2.0);
        let moderate = CreatureGenome::BASE.with(Gene::Territoriality, 1.0);
        // many flocks stacked at nearly the same place near the surface
        for i in 0..12 {
            let genome = if i % 2 == 0 { hard } else { moderate };
            flock_at(&mut w, genome.with(Gene::FlockKind, 1.0), 9, 3000.0 + i as f64 * 7.0, 400.0);
        }
        for _ in 0..400 {
            update(&mut w.flocks, &mut w.creatures, &w.space, 1, true);
            // members stay with their circles in this test
            for v in &mut w.creatures {
                let c = v.circle.unwrap();
                (v.x, v.y) = (c.x, c.y);
            }
            let stats = overlap_stats(&w.flocks, &w.space);
            assert_eq!(stats.strict_pairs, 0, "жёсткие круги перекрылись: {stats:?}");
        }
    }

    #[test]
    fn умеренные_расходятся_постепенно_а_без_территории_перекрываются() {
        for (mode, apart) in [(1.0, true), (0.0, false)] {
            let mut w = world();
            let g = CreatureGenome::BASE.with(Gene::Territoriality, mode);
            let a = flock_at(&mut w, g, 4, 3000.0, 1500.0);
            let b = flock_at(&mut w, g, 4, 3050.0, 1500.0);
            update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
            // the second circle is placed free of the first when there is a rule between them
            let gap = |w: &World| {
                let (p, q) = (w.flocks[&a].circle.unwrap(), w.flocks[&b].circle.unwrap());
                p.radius + q.radius - (p.x - q.x).hypot(p.y - q.y)
            };
            assert_eq!(gap(&w) <= OVERLAP_TOLERANCE, apart, "режим {mode}");
        }
    }

    #[test]
    fn оседлые_стоят_а_кочевые_идут_и_разворачиваются_у_края() {
        let mut w = world();
        let settled = flock_at(&mut w, CreatureGenome::BASE, 4, 1000.0, 1500.0);
        let nomad = flock_at(&mut w, CreatureGenome::BASE.with(Gene::FlockKind, 1.0), 4, 5600.0, 3000.0);
        update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
        let home = w.flocks[&settled].circle.unwrap();
        let mut xs = Vec::new();
        for _ in 0..3000 {
            // feed the settled flock so it does not move
            for v in w.creatures.iter_mut().filter(|v| v.flock == settled) {
                v.mind.social.personal_food = Some((v.x, v.y));
            }
            update(&mut w.flocks, &mut w.creatures, &w.space, 1, true);
            for v in &mut w.creatures {
                let c = v.circle.unwrap();
                (v.x, v.y) = (c.x, c.y);
            }
            xs.push(w.flocks[&nomad].circle.unwrap().x);
        }
        assert_eq!(w.flocks[&settled].circle.unwrap(), home);
        let speed = CIRCLE_PACE * 10.0;
        assert!(xs.windows(2).all(|p| (p[1] - p[0]).abs() <= speed + 1e-9), "кочевой круг прыгнул");
        let turns = xs.windows(3).filter(|p| (p[1] - p[0]) * (p[2] - p[1]) < 0.0).count();
        assert!(turns >= 1, "кочевые развернулись у края мира");
        let span = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            - xs.iter().cloned().fold(f64::INFINITY, f64::min);
        assert!(span > 2000.0, "кочевые прошли заметный путь: {span}");
    }

    #[test]
    fn голодные_оседлые_переезжают_на_свободное_место() {
        let mut w = world();
        let tag = flock_at(&mut w, CreatureGenome::BASE, 4, 3000.0, 1500.0);
        update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
        let home = w.flocks[&tag].circle.unwrap();
        for v in &mut w.creatures {
            v.energy = v.pheno.max_energy * 0.2;
        }
        let mut moved = 0;
        for _ in 0..(HUNGRY_TICKS as usize + 400) {
            moved += update(&mut w.flocks, &mut w.creatures, &w.space, 1, true);
            for v in &mut w.creatures {
                let c = v.circle.unwrap();
                (v.x, v.y) = (c.x, c.y);
            }
        }
        assert!(moved >= 1);
        let now = w.flocks[&tag].circle.unwrap();
        assert!((now.x - home.x).hypot(now.y - home.y) > home.radius, "круг переехал");
    }

    #[test]
    fn мигранты_ходят_по_глубине_с_периодом() {
        let mut w = world();
        let tag = flock_at(&mut w, CreatureGenome::BASE.with(Gene::FlockKind, 3.0), 4, 3000.0, 2000.0);
        let mut ys = Vec::new();
        for _ in 0..(2 * MIGRATION_PERIOD) {
            update(&mut w.flocks, &mut w.creatures, &w.space, 1, true);
            for v in &mut w.creatures {
                let c = v.circle.unwrap();
                (v.x, v.y) = (c.x, c.y);
            }
            ys.push(w.flocks[&tag].circle.unwrap().y);
        }
        let (lo, hi) = ys.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &y| (a.min(y), b.max(y)));
        assert!(hi - lo > 1500.0, "мигранты прошли по глубине: {lo:.0}..{hi:.0}");
        let turns = ys.windows(3).filter(|p| (p[1] - p[0]) * (p[2] - p[1]) < 0.0).count();
        assert!((2..=6).contains(&turns), "развороты по глубине: {turns}");
    }

    #[test]
    fn разведчики_едут_к_находке_без_скачков() {
        let mut w = world();
        let tag = flock_at(&mut w, CreatureGenome::BASE.with(Gene::FlockKind, 2.0), 4, 1000.0, 1500.0);
        update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
        let observer = w.creatures[0].id;
        let mut last = w.flocks[&tag].circle.unwrap();
        for t in 0..1200u64 {
            w.creatures[0].mind.social.observed_food =
                Some(crate::social::Food { x: 2500.0, y: 1500.0, tick: t, observer });
            food_goals(&mut w.flocks, &w.creatures, t);
            update(&mut w.flocks, &mut w.creatures, &w.space, 1, true);
            for v in &mut w.creatures {
                let c = v.circle.unwrap();
                (v.x, v.y) = (c.x, c.y);
            }
            let now = w.flocks[&tag].circle.unwrap();
            assert!((now.x - last.x).hypot(now.y - last.y) <= CIRCLE_PACE * 10.0 + 1e-9);
            last = now;
        }
        assert!((last.x - 2500.0).abs() < 5.0, "разведчики дошли до находки: {}", last.x);
    }

    #[test]
    fn круг_ждёт_отставших() {
        let mut w = world();
        let tag = flock_at(&mut w, CreatureGenome::BASE.with(Gene::FlockKind, 1.0), 4, 3000.0, 1500.0);
        update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
        let start = w.flocks[&tag].circle.unwrap();
        for v in &mut w.creatures {
            v.x += 1000.0;
        }
        for _ in 0..10 {
            update(&mut w.flocks, &mut w.creatures, &w.space, 1, true);
        }
        assert_eq!(w.flocks[&tag].circle.unwrap(), start);
    }

    #[test]
    fn a_young_family_follows_its_members_with_a_circle_as_wide_as_they_see() {
        let mut w = world();
        let tag = flock_at(&mut w, CreatureGenome::BASE, 2, 3000.0, 1500.0);
        update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
        let f = &w.flocks[&tag];
        assert!(f.young());
        let c = f.circle.unwrap();
        assert!(c.radius >= f.vision.min(MAX_RADIUS) - 1e-9, "{} < {}", c.radius, f.vision);
        for v in &mut w.creatures {
            v.x += 500.0;
        }
        let mut last = c;
        for _ in 0..200 {
            update(&mut w.flocks, &mut w.creatures, &w.space, 1, true);
            let now = w.flocks[&tag].circle.unwrap();
            assert!((now.x - last.x).hypot(now.y - last.y) <= w.flocks[&tag].speed + 1e-9, "no jumps");
            last = now;
        }
        let f = &w.flocks[&tag];
        assert!(
            (last.x - f.center.0).hypot(last.y - f.center.1) < 1.0,
            "the circle caught up with the family"
        );
    }

    #[test]
    fn crowded_hard_flocks_get_cornered_only_with_combat() {
        for combat in [true, false] {
            let mut w = world();
            // a thin layer: every circle stands on one line
            let g = CreatureGenome::BASE
                .with(Gene::Territoriality, 2.0)
                .with(Gene::FlockSpacing, 500.0)
                .with(Gene::MinY, 10.0)
                .with(Gene::MaxY, 10.0);
            for i in 0..12 {
                flock_at(&mut w, g, 4, 500.0 + i as f64 * 450.0, 600.0);
            }
            let (mut cornered, mut moves) = (false, 0);
            for _ in 0..300 {
                moves += update_full(&mut w.flocks, &mut w.creatures, &w.space, 1, true, combat);
                cornered |= w.flocks.values().any(|f| f.cornered);
                for v in &mut w.creatures {
                    let c = v.circle.unwrap();
                    (v.x, v.y) = (c.x, c.y);
                }
                assert_eq!(overlap_stats(&w.flocks, &w.space).strict_pairs, 0);
            }
            assert_eq!(cornered, combat, "combat {combat}");
            // while there is room nearby, both move; without combat a squeezed flock only moves
            assert!(combat || moves > 0, "without combat the squeezed move instead");
        }
    }

    #[test]
    fn with_no_room_at_the_surface_a_free_place_is_deeper() {
        let mut w = world();
        let g = CreatureGenome::BASE.with(Gene::Territoriality, 2.0).with(Gene::FlockSpacing, 500.0);
        let mut tags = Vec::new();
        for i in 0..5 {
            tags.push(flock_at(&mut w, g, 4, 600.0 + i as f64 * 1200.0, 600.0));
        }
        update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
        for (i, tag) in tags.iter().enumerate() {
            w.flocks.get_mut(tag).unwrap().circle =
                Some(Circle { x: 600.0 + i as f64 * 1200.0, y: 600.0, radius: 600.0 });
        }
        let circles = Circles::of(&w.flocks, &w.space);
        let bounds = Bounds { x0: 600.0, x1: 5400.0, y0: 600.0, y1: 3400.0 };
        let mut rng = Rng::new(5);
        let spot = circles.free_spot(
            u64::MAX,
            (3000.0, 600.0),
            None,
            None,
            600.0,
            Territoriality::Hard,
            bounds,
            &mut rng,
        );
        assert!(circles.conflict(u64::MAX, spot.0, spot.1, 600.0, Territoriality::Hard) <= OVERLAP_TOLERANCE);
        assert!(spot.1 > 600.0 + 1.0, "deeper than the full surface: {spot:?}");
    }

    #[test]
    fn births_never_leave_hard_circles_overlapping() {
        let mut w = world();
        let hard = CreatureGenome::BASE.with(Gene::Territoriality, 2.0).with(Gene::FlockSpacing, 300.0);
        let mut tags = Vec::new();
        for i in 0..6 {
            tags.push(flock_at(&mut w, hard, 4, 2000.0 + i as f64 * 300.0, 1500.0));
        }
        for _ in 0..50 {
            update(&mut w.flocks, &mut w.creatures, &w.space, 1, true);
        }
        assert_eq!(overlap_stats(&w.flocks, &w.space).strict_pairs, 0);
        // each flock doubles: its nominal circle grows by √2
        for tag in tags {
            let c = w.flocks[&tag].circle.unwrap();
            for k in 0..4 {
                w.spawn(hard, c.x + k as f64, c.y, Some(100.0));
                w.creatures.last_mut().unwrap().flock = tag;
            }
        }
        update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
        assert_eq!(overlap_stats(&w.flocks, &w.space).strict_pairs, 0);
    }

    #[test]
    fn отставший_уходит_с_новой_меткой() {
        let mut w = world();
        let tag = flock_at(&mut w, CreatureGenome::BASE, 3, 3000.0, 1500.0);
        update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
        w.creatures[0].mind.social.outside_since = Some(100);
        let mut next = 1000;
        let mut transitions = Vec::new();
        assert_eq!(stragglers(&mut w.creatures, &mut next, 399, &mut transitions), 0);
        assert_eq!(stragglers(&mut w.creatures, &mut next, 400, &mut transitions), 1);
        assert_eq!(w.creatures[0].flock, 1000);
        assert_eq!(transitions, vec![(tag, 1000)]);
        assert!(w.creatures[1..].iter().all(|v| v.flock == tag));
    }
}
