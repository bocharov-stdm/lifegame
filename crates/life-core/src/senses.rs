//! The senses: what a creature can learn about the world.
//!
//! Creatures do not see the neighbour grid: they ask «where is the nearest plant», «who nearby
//! can eat me» — through a trait. The world answers by the grids (`GridSenses` below, built for
//! each creature), the tests by plain closures (`senses_from`) or by blindness (`Blind`).
//!
//! A new sense is a trait method, a query function below and its check against a brute-force
//! search in this module's test.
//!
//! Creatures see their own kind only by the snapshot at the start of the phase (`Herd`). In
//! their phase they move: the grid's copies of coordinates would go stale, and reading live
//! positions would make the outcome depend on the order of traversal. By the snapshot everyone
//! sees the neighbours where they stood at the start of the tick (a lag of no more than a
//! step), and the parallel tick will be able to decide for all at once (CLAUDE.md,
//! «Neighbour search»).

use crate::config::GRID_CELL;
use crate::corpse::Corpse;
use crate::creature::{Creature, Kinship, Me, Sighting};
use crate::grid::Grid;
use crate::plant::Plant;
use crate::space::Space;

/// What a creature takes for food this tick, as its program's settings set it (`Stance`): the
/// other niche's food too, and how far past its layer (`layer`, y from and to) it goes for it (y;
/// infinite: anywhere).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Taste {
    pub foreign: bool,
    pub reach: f64,
    pub layer: (f64, f64),
}

impl Taste {
    /// Only its own food, anywhere.
    pub const OWN: Taste = Taste { foreign: false, reach: f64::INFINITY, layer: (0.0, 0.0) };

    /// Food at depth `y` is within its reach of its layer.
    #[inline]
    pub fn admits(&self, y: f64) -> bool {
        y >= self.layer.0 - self.reach && y <= self.layer.1 + self.reach
    }
}

/// The terms of a hunt block (`Action::Hunt`): prey this many times smaller, the weight of the
/// strikes it expects (1: the old base caution), what it takes for food, and how far it looks for
/// prey (no farther than its sight).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hunting {
    pub ratio: f64,
    pub caution: f64,
    pub taste: Taste,
    pub range: f64,
}

/// What a plant with `portions` left at (px, py) adds to `me`'s tank per tick of the way and the
/// meal: its plant choice and its weighing of other food read it.
#[inline]
pub(crate) fn plant_worth(me: &Me, px: f64, py: f64, portions: u8) -> f64 {
    let portions = f64::from(portions);
    let bite = me.pheno.plant_energy * me.pheno.plant_bite_yield * me.pheno.plant_efficiency
        / f64::from(crate::plant::PORTIONS);
    let room = (me.pheno.max_energy - me.energy).max(0.0);
    (bite * portions).min(room)
        / (((px - me.x).hypot(py - me.y) - me.pheno.size).max(0.0) / me.pheno.speed.max(0.01) + portions)
            .max(1.0)
}

/// A creature's senses.
pub trait Senses {
    fn visible_enemy(&self, _me: &Me, _id: u64) -> Option<Threat> {
        None
    }
    /// The nearest live plant strictly closer than √r2.
    fn nearest_plant(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64)>;

    /// The nearest live plant strictly closer than √r2 whose position `keep` accepts. The default
    /// filters `nearest_plant`, enough for test senses.
    fn nearest_plant_where(
        &self,
        x: f64,
        y: f64,
        r2: f64,
        keep: impl Fn(f64, f64) -> bool,
    ) -> Option<(f64, f64)> {
        self.nearest_plant(x, y, r2).filter(|&(px, py)| keep(px, py))
    }

    /// Portions left on the live plant at exactly (x, y), as `nearest_plant` returned it. The
    /// default — a whole plant — is enough for test senses.
    fn plant_portions(&self, _x: f64, _y: f64) -> u8 {
        crate::plant::PORTIONS
    }

    /// The best corpse it smells and eats at `taste`, weighing the way and the meal.
    fn best_corpse(&self, _me: &Me, _taste: Taste) -> Option<CorpseFood> {
        None
    }

    /// The nearest stranger (not kin) that can eat me and whose body's edge is closer than
    /// `within` — by the herd's snapshot at the start of the phase.
    fn nearest_threat(&self, me: &Me, within: f64) -> Option<Threat>;

    /// The nearest threat and the nearest one hunting somebody now, closer than `within`. The
    /// default takes every threat for a hunter, enough for test senses.
    fn threats_near(&self, me: &Me, within: f64) -> (Option<Threat>, Option<Threat>) {
        let t = self.nearest_threat(me, within);
        (t, t)
    }

    /// The prey worth most at the hunt's terms, in sight; the previous target first while it is
    /// allowed. `avoid`: a prey the hunter gave up chasing, not a candidate.
    fn prey(&self, _me: &Me, _previous: Option<u64>, _avoid: Option<u64>, _hunt: Hunting) -> Option<Prey> {
        None
    }

    /// The nearest stranger it could eat at `taste` — at least `ratio` times smaller, within its
    /// layer's reach — closer than `within` to its body's edge (`Cond::PreySeen`).
    fn nearest_prey(&self, _me: &Me, _ratio: f64, _taste: Taste, _within: f64) -> Option<Threat> {
        None
    }

    /// Its child in need within `within` of it: the child it knows that was struck within `window`
    /// ticks of `tick`, or while young met a threat on the last tick, and the enemy that did it,
    /// if that one is in its sight and no family of its own — the child `prefer` first, then the
    /// nearest (`Action::DefendChild`).
    fn child_in_need(
        &self,
        _me: &Me,
        _within: f64,
        _tick: u64,
        _window: u64,
        _prefer: Option<u64>,
    ) -> Option<(u64, Threat)> {
        None
    }

    /// The live plant in sight within its reach at `taste` worth most per tick of the way and the
    /// meal (`plant_worth`), the nearer of equals. The default — the nearest — is enough for test
    /// senses.
    fn best_plant(&self, me: &Me, taste: Taste) -> Option<(f64, f64)> {
        self.nearest_plant_where(me.x, me.y, me.pheno.vision2, |_, y| taste.admits(y))
    }
}

/// An estimate of prey only by a personally seen creature.
#[derive(Clone, Copy, Debug)]
pub struct Prey {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    /// Radius of its body: a chase is measured to its edge.
    pub half: f64,
    pub score: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct CorpseFood {
    pub owner: u64,
    pub x: f64,
    pub y: f64,
    /// Radius of the body it was: a corpse is reached across it.
    pub half: f64,
    pub score: f64,
}

impl Prey {
    /// Energy per tick the hunt is worth to `me`: the meat it can still take in (its tank is
    /// not bottomless), less the share `caution` gives up for the strikes it expects.
    /// The prey strikes back while it is being killed as its program does: only a hunter within
    /// its fight-back block's size, and only while its health holds above that block's threshold;
    /// its visible allies (`allies`: the sum of their strikes) join in until the hunter has eaten.
    /// A careless hunter (caution 0) ignores the risk; a hunt that looks deadly for a careful one
    /// is worth nothing.
    fn of(s: &Seen, me: &Me, allies: f64, caution: f64) -> Self {
        let travel =
            ((s.x - me.x).hypot(s.y - me.y) - s.half - me.pheno.half).max(0.0) / me.pheno.speed.max(0.01);
        let hits = (s.health / me.pheno.strike_on(s.half * 2.0).max(0.001)).ceil();
        let portion = (me.pheno.plant_energy / f64::from(crate::plant::PORTIONS)).max(s.nutrition / 12.0);
        let feeding = (s.nutrition / portion.max(0.001)).ceil();
        let gain = (s.nutrition * me.pheno.meat_efficiency).min(me.pheno.max_energy - me.energy).max(0.0);
        let back = if me.pheno.half < s.fights_below { (hits * s.fights_share).ceil() } else { 0.0 };
        let expected = s.strike_on(me) * back + allies * (hits + feeding);
        let risk = caution * expected / me.health.max(0.001);
        Self {
            id: s.kinship.id,
            x: s.x,
            y: s.y,
            half: s.half,
            score: gain * (1.0 - risk).max(0.0) / (travel + hits + feeding).max(1.0),
        }
    }
}

/// How many prey candidates a hunter keeps in mind at once. The buffer lives on the stack. A full
/// buffer keeps the nearest candidates (ties by id) and the current target, so what the hunter
/// weighs does not depend on the grid's scan order.
const SEEN_PREY: usize = 64;

/// A dangerous stranger as a creature sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Threat {
    pub id: u64,
    /// The centre of its body.
    pub x: f64,
    pub y: f64,
    /// The distance to the edge of its body; inside the body — less than zero.
    pub gap: f64,
    /// The radius of its body: the strongest is knowingly not struck back at, one runs from it.
    pub half: f64,
}

/// The world through a creature's eyes: plants by the tick's grid, relatives by the herd's snapshot.
///
/// The queries and the senses' methods are marked `#[inline(always)]`: without it the compiler
/// did not inline the plant search into the creature's move, and a ×100 world ran 8% slower
/// than with the former closures (measured against the version before the senses).
pub(crate) struct GridSenses<'a> {
    pub food: &'a Grid,
    pub plants: &'a [Plant],
    /// The corpse grid; None — no corpses to look for (tests of the other queries).
    pub corpse_grid: Option<&'a Grid>,
    pub corpses: &'a [Corpse],
    pub now: u64,
    /// The snapshot of the herd; None — nobody to hunt or fear (tests of the corpse queries).
    pub herd: Option<&'a Herd>,
}

impl Senses for GridSenses<'_> {
    #[inline(always)]
    fn best_corpse(&self, me: &Me, taste: Taste) -> Option<CorpseFood> {
        if !me.pheno.eats_corpses() {
            return None; // every corpse would be worth nothing to it
        }
        let mut best: Option<CorpseFood> = None;
        // what it can still take in: a corpse bigger than the empty part of its tank is worth no more
        let room = (me.pheno.max_energy - me.energy).max(0.0);
        self.corpse_grid?.for_each_near(me.x, me.y, me.pheno.smell, |i, cx, cy| {
            let c = &self.corpses[i];
            let distance = (cx - me.x).hypot(cy - me.y);
            let efficiency = me.pheno.corpse_efficiency(c.stage(self.now), taste.foreign);
            if c.born >= self.now
                || c.remaining <= 0.0
                || distance >= me.pheno.smell
                || efficiency <= 0.0
                || !taste.admits(cy)
            {
                return;
            }
            let portion = c.portion(me.pheno.plant_energy);
            let feeding = (c.remaining / portion).ceil();
            let travel = (distance - me.pheno.size - c.size * 0.5).max(0.0) / me.pheno.speed.max(0.01);
            let score = (c.remaining * efficiency).min(room) / (travel + feeding).max(1.0);
            let candidate = CorpseFood { owner: c.owner, x: cx, y: cy, half: c.size * 0.5, score };
            if best.is_none_or(|b| score > b.score || (score == b.score && c.owner < b.owner)) {
                best = Some(candidate);
            }
        });
        best
    }

    fn visible_enemy(&self, me: &Me, id: u64) -> Option<Threat> {
        let herd = self.herd?;
        let i = herd.seen.binary_search_by_key(&id, |s| s.kinship.id).ok()?;
        let s = &herd.seen[i];
        let distance = (s.x - me.x).hypot(s.y - me.y);
        if me.kinship.kin(s.kinship) || distance > me.pheno.vision {
            return None;
        }
        Some(Threat { id, x: s.x, y: s.y, gap: distance - s.half, half: s.half })
    }
    fn prey(&self, me: &Me, previous: Option<u64>, avoid: Option<u64>, hunt: Hunting) -> Option<Prey> {
        if !me.pheno.hunts_now(hunt.taste.foreign) {
            return None; // fresh meat is worth nothing to it, or meat is not its own this tick
        }
        let herd = self.herd?;
        let max_size = me.pheno.size / hunt.ratio;
        let range = hunt.range.min(me.pheno.vision);
        // (kept first, distance², id, index): the order in which a full buffer keeps candidates
        let mut candidates = [(false, 0.0_f64, 0_u64, 0_usize); SEEN_PREY];
        let mut n_candidates = 0;
        let order = |a: &(bool, f64, u64, usize), b: &(bool, f64, u64, usize)| {
            a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2))
        };
        // the one a full buffer gives up next: found again only when it has been replaced
        let mut worst = 0;
        herd.grid.for_each_near(me.x, me.y, range, |j, _, _| {
            let s = &herd.seen[j];
            let d2 = (s.x - me.x).powi(2) + (s.y - me.y).powi(2);
            if d2.sqrt() > range || s.kinship.id == me.kinship.id {
                return;
            }
            if s.half * 2.0 > max_size
                || me.kinship.kin(s.kinship)
                || Some(s.kinship.id) == avoid
                // past its layer's reach, as plants and corpses (`Action::Reach`)
                || !hunt.taste.admits(s.y)
            {
                return;
            }
            let key = (Some(s.kinship.id) != previous, d2, s.kinship.id, j);
            if n_candidates < SEEN_PREY {
                candidates[n_candidates] = key;
                n_candidates += 1;
                if n_candidates < SEEN_PREY {
                    return;
                }
            } else if order(&key, &candidates[worst]).is_lt() {
                candidates[worst] = key;
            } else {
                return;
            }
            worst = (0..SEEN_PREY).max_by(|&a, &b| order(&candidates[a], &candidates[b])).unwrap();
        });
        let mut best: Option<Prey> = None;
        for &(_, _, _, j) in &candidates[..n_candidates] {
            let s = &herd.seen[j];
            // A parent in sight that still knows it and whose program defends its children.
            let mut allies = 0.0;
            if let Ok(k) = herd.seen.binary_search_by_key(&s.kinship.parent, |p| p.kinship.id) {
                let p = &herd.seen[k];
                if p.defends && p.kinship.kin(s.kinship) && (p.x - me.x).hypot(p.y - me.y) <= me.pheno.vision
                {
                    allies += p.strike_on(me);
                }
            }
            let p = Prey::of(s, me, allies, hunt.caution);
            let kept = |b: &Prey| Some(b.id) == previous && b.score > 0.0;
            if best.is_none_or(|b| {
                !kept(&b) && (kept(&p) || p.score > b.score || (p.score == b.score && p.id < b.id))
            }) {
                best = Some(p);
            }
        }
        best
    }

    fn nearest_prey(&self, me: &Me, ratio: f64, taste: Taste, within: f64) -> Option<Threat> {
        nearest_prey(self.herd?, me, ratio, taste, within)
    }

    fn child_in_need(
        &self,
        me: &Me,
        within: f64,
        tick: u64,
        window: u64,
        prefer: Option<u64>,
    ) -> Option<(u64, Threat)> {
        child_in_need(self.herd?, me, within, tick, window, prefer)
    }

    fn best_plant(&self, me: &Me, taste: Taste) -> Option<(f64, f64)> {
        best_plant(self.food, self.plants, me, taste)
    }

    #[inline(always)]
    fn nearest_plant(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64)> {
        nearest_plant(self.food, self.plants, x, y, r2)
    }

    fn plant_portions(&self, x: f64, y: f64) -> u8 {
        let mut portions = 0;
        self.food.for_each_near(x, y, 1.0, |j, px, py| {
            if (px, py) == (x, y) {
                portions = portions.max(self.plants[j].portions);
            }
        });
        portions
    }

    #[inline(always)]
    fn nearest_plant_where(
        &self,
        x: f64,
        y: f64,
        r2: f64,
        keep: impl Fn(f64, f64) -> bool,
    ) -> Option<(f64, f64)> {
        nearest_plant_where(self.food, self.plants, x, y, r2, keep)
    }

    #[inline(always)]
    fn nearest_threat(&self, me: &Me, within: f64) -> Option<Threat> {
        self.threats_near(me, within).0
    }

    #[inline(always)]
    fn threats_near(&self, me: &Me, within: f64) -> (Option<Threat>, Option<Threat>) {
        match self.herd {
            Some(herd) => nearest_threats(herd, me.kinship, me.x, me.y, me.pheno.size, within),
            None => (None, None),
        }
    }
}

/// Senses for tests: a plant from the closure `senses_from(|x, y, r2| ..)`, a threat — a given
/// one (`with_threat`; visible if closer than asked) or none.
pub struct FnSenses<F> {
    plant: F,
    threat: Option<Threat>,
}

/// For a creature: the nearest plant; no threats.
pub fn senses_from<F>(plant: F) -> FnSenses<F>
where
    F: Fn(f64, f64, f64) -> Option<(f64, f64)>,
{
    FnSenses { plant, threat: None }
}

impl<F> FnSenses<F> {
    /// The same senses, but a dangerous stranger nearby.
    pub fn with_threat(self, threat: Threat) -> Self {
        FnSenses { threat: Some(threat), ..self }
    }
}

impl<F> Senses for FnSenses<F>
where
    F: Fn(f64, f64, f64) -> Option<(f64, f64)>,
{
    fn nearest_plant(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64)> {
        (self.plant)(x, y, r2)
    }

    fn nearest_threat(&self, _: &Me, within: f64) -> Option<Threat> {
        self.threat.filter(|t| t.gap < within)
    }
}

/// Sees nothing.
pub struct Blind;

impl Senses for Blind {
    fn nearest_plant(&self, _: f64, _: f64, _: f64) -> Option<(f64, f64)> {
        None
    }

    fn nearest_threat(&self, _: &Me, _: f64) -> Option<Threat> {
        None
    }
}

// ── the herd's snapshot ─────────────────────────────────────────────────────

/// A creature in the herd's snapshot: what of it is visible to the others.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Seen {
    x: f64,
    y: f64,
    /// The body's radius: to a stranger the distance to the body's edge is measured, not to the centre.
    half: f64,
    /// The biggest body it can eat; 0 — it eats no fresh meat and is a threat to no one.
    eats_up_to: f64,
    health: f64,
    nutrition: f64,
    /// Its melee strike: what a hunter expects back from it or from it as an ally.
    strike: f64,
    /// Its defence (`Menace::fight`): it strikes back an enemy with a smaller radius than this
    /// (0: nobody), for this share of the strikes that kill it (down to its block's health
    /// threshold). A hunter weighs what the prey did on its last move, as the prey fears the
    /// hunter's hunt block that its tests let through.
    fights_below: f64,
    fights_share: f64,
    kinship: Kinship,
    /// It chose a target on its last move: it is hunting (or fighting) someone.
    hunting: bool,
    /// Its defence block's tests held on its last move (`Menace::defends`): a hunter counts it as
    /// its children's ally.
    defends: bool,
    /// The enemy that struck it last, and — while it is young — the threat it met last: what its
    /// parent sees of its need (`child_in_need`).
    hit: Option<Sighting>,
    alarm: Option<Sighting>,
}

impl Seen {
    /// Its strike on `me`: harder when it is the bigger (`phenotype::melee_damage`).
    #[inline]
    fn strike_on(&self, me: &Me) -> f64 {
        crate::creature::melee_damage(self.strike, self.half * 2.0, me.pheno.size, me.pheno.melee_size_power)
    }
}

/// A snapshot of all creatures at the start of the phase: the small are needed as prey, the big as threats.
/// The queries use the grid; the buffers live between ticks.
#[derive(Clone, Debug)]
pub(crate) struct Herd {
    grid: Grid,
    seen: Vec<Seen>,
    /// Only those that could eat somebody (`Seen::eats_up_to` > 0), for the threat queries: in a
    /// crowd of grazers a threat query looks at the few hunters, not at every neighbour. The grid's
    /// indices are into `hunters`, which holds indices into `seen`, in `seen`'s order.
    hunter_grid: Grid,
    hunters: Vec<usize>,
    /// Indices into `seen` ordered by parent: a parent looks only at its own children.
    by_parent: Vec<usize>,
    /// The biggest body anyone can eat. One bigger has nothing to fear, and does not look
    /// into the grid.
    max_eats: f64,
    /// The biggest body radius in the snapshot: the query is wider by it.
    max_half: f64,
    /// The biggest body radius among the hunters: the threat query is wider by it.
    max_hunter_half: f64,
}

impl Herd {
    pub fn new() -> Self {
        Herd {
            grid: Grid::new(GRID_CELL),
            seen: Vec::new(),
            hunter_grid: Grid::new(GRID_CELL),
            hunters: Vec::new(),
            by_parent: Vec::new(),
            max_eats: 0.0,
            max_half: 0.0,
            max_hunter_half: 0.0,
        }
    }

    /// A snapshot of the creatures as they stand now. At the start of the phase all are alive:
    /// the dead are swept at the end of the previous one. The ones looking into the snapshot are
    /// the ones in it: children are born after the moves. Whom one may eat, how it fights back and
    /// whether it covers its children is what its blocks did on its last move (`Creature::menace`).
    #[cfg(test)]
    pub fn rebuild(&mut self, space: &Space, creatures: &[Creature]) {
        self.rebuild_on(space, creatures, &crate::par::Threads::One);
    }

    /// The snapshot, each creature's entry on `threads`.
    pub fn rebuild_on(&mut self, space: &Space, creatures: &[Creature], threads: &crate::par::Threads) {
        debug_assert!(creatures.iter().all(|v| v.alive), "в снимке стада мёртвые");
        crate::par::map_into(threads, creatures, &mut self.seen, |v| {
            let menace = v.menace();
            let (fights_below, fights_share) =
                menace.fight().map_or((0.0, 0.0), |(ratio, health)| (v.pheno.half * ratio, 1.0 - health));
            Seen {
                x: v.x,
                y: v.y,
                half: v.pheno.half,
                eats_up_to: match menace.hunt() {
                    Some(ratio) if v.pheno.hunts() => v.pheno.size / ratio,
                    _ => 0.0,
                },
                health: v.health,
                nutrition: crate::corpse::meat(v),
                strike: v.pheno.strike(),
                fights_below,
                fights_share,
                kinship: v.kinship(),
                hunting: v.mind.attack.is_some(),
                defends: menace.defends,
                hit: v.mind.hit,
                alarm: if v.adult() { None } else { v.mind.alarm },
            }
        });
        self.grid.rebuild(space, self.seen.iter().map(|s| (s.x, s.y)));
        let (mut max_eats, mut max_half, mut max_hunter_half) = (0.0_f64, 0.0_f64, 0.0_f64);
        self.hunters.clear();
        for (i, s) in self.seen.iter().enumerate() {
            max_eats = max_eats.max(s.eats_up_to);
            max_half = max_half.max(s.half);
            if s.eats_up_to > 0.0 {
                self.hunters.push(i);
                max_hunter_half = max_hunter_half.max(s.half);
            }
        }
        (self.max_eats, self.max_half, self.max_hunter_half) = (max_eats, max_half, max_hunter_half);
        let seen = &self.seen;
        self.hunter_grid.rebuild(space, self.hunters.iter().map(|&i| (seen[i].x, seen[i].y)));
        self.by_parent.clear();
        self.by_parent.extend(0..seen.len());
        self.by_parent.sort_unstable_by_key(|&i| (seen[i].kinship.parent, i));
    }

    /// The indices into `seen` of `parent`'s children.
    fn children_of(&self, parent: u64) -> &[usize] {
        let from = self.by_parent.partition_point(|&i| self.seen[i].kinship.parent < parent);
        let to = self.by_parent.partition_point(|&i| self.seen[i].kinship.parent <= parent);
        &self.by_parent[from..to]
    }
}

// ── queries to the grids ────────────────────────────────────────────────────
// Taken out of the tick so that a test can check them against an honest brute-force search in
// a live world: an error in a query's radius crashes nothing, but quietly changes the balance
// — creatures stop noticing neighbours under their noses.

/// The nearest strangers (not family of `who`) from the snapshot, measured to the edge of their
/// bodies, that could eat a body of `size` and are closer than `within`: the nearest of all, and
/// the nearest hunting somebody (it chose a target on its last move). How near a calm one may come
/// is the program's choice (`Cond::ThreatNear`, `Cond::HunterNear`).
#[inline(always)]
pub(crate) fn nearest_threats(
    herd: &Herd,
    who: Kinship,
    x: f64,
    y: f64,
    size: f64,
    within: f64,
) -> (Option<Threat>, Option<Threat>) {
    if size > herd.max_eats {
        return (None, None); // nobody in the world can eat such a body
    }
    let (mut best, mut hunter): (Option<Threat>, Option<Threat>) = (None, None);
    // only those that eat anybody: the rest could not be threats (`eats_up_to` 0), and the order
    // among the hunters is the whole herd's order
    herd.hunter_grid.for_each_near(x, y, within + herd.max_hunter_half, |h, sx, sy| {
        let s = &herd.seen[herd.hunters[h]];
        if size > s.eats_up_to || who.kin(s.kinship) {
            return;
        }
        let (dx, dy) = (sx - x, sy - y);
        let d2 = dx * dx + dy * dy;
        let reach = within + s.half;
        if d2 >= reach * reach {
            return;
        }
        let gap = d2.sqrt() - s.half;
        let t = Threat { id: s.kinship.id, x: sx, y: sy, gap, half: s.half };
        if best.is_none_or(|b| gap < b.gap) {
            best = Some(t);
        }
        if s.hunting && hunter.is_none_or(|b| gap < b.gap) {
            hunter = Some(t);
        }
    });
    (best, hunter)
}

/// The nearest stranger from the snapshot `me` could eat at `taste` — at least `ratio` times
/// smaller (as its hunt measures prey: the diameter against its size over the ratio), not family,
/// within its layer's reach — closer than `within` to its body's edge.
pub(crate) fn nearest_prey(herd: &Herd, me: &Me, ratio: f64, taste: Taste, within: f64) -> Option<Threat> {
    if !me.pheno.hunts_now(taste.foreign) {
        return None;
    }
    let max_size = me.pheno.size / ratio;
    let mut best: Option<Threat> = None;
    herd.grid.for_each_near(me.x, me.y, within + herd.max_half, |j, sx, sy| {
        let s = &herd.seen[j];
        if s.kinship.id == me.kinship.id
            || s.half * 2.0 > max_size
            || me.kinship.kin(s.kinship)
            || !taste.admits(sy)
        {
            return;
        }
        let gap = (sx - me.x).hypot(sy - me.y) - s.half;
        if gap < within && best.is_none_or(|b| gap < b.gap || (gap == b.gap && s.kinship.id < b.id)) {
            best = Some(Threat { id: s.kinship.id, x: sx, y: sy, gap, half: s.half });
        }
    });
    best
}

/// Its child in need from the snapshot (`Senses::child_in_need`): a child `me` knows within
/// `within` of it, struck within `window` ticks of `tick` or, while young, frightened on the last
/// tick (the later of the two), whose enemy stands in its sight and is no family of its own; the
/// child `prefer` first, then the nearest, ties by id.
pub(crate) fn child_in_need(
    herd: &Herd,
    me: &Me,
    within: f64,
    tick: u64,
    window: u64,
    prefer: Option<u64>,
) -> Option<(u64, Threat)> {
    let mut best: Option<(bool, f64, u64, Threat)> = None;
    // only its own children: the choice below is by a full order, so no scan order matters
    for &j in herd.children_of(me.kinship.id) {
        let s = &herd.seen[j];
        let (sx, sy) = (s.x, s.y);
        if s.kinship.id == me.kinship.id || !me.kinship.kin(s.kinship) {
            continue;
        }
        let d = (sx - me.x).hypot(sy - me.y);
        if d > within {
            continue;
        }
        let struck = s.hit.filter(|h| tick.saturating_sub(h.tick) <= window);
        let frightened = s.alarm.filter(|a| tick.saturating_sub(a.tick) <= 1);
        let Some(need) = [struck, frightened].into_iter().flatten().max_by_key(|e| e.tick) else { continue };
        let Ok(k) = herd.seen.binary_search_by_key(&need.enemy, |u| u.kinship.id) else { continue };
        let e = &herd.seen[k];
        let distance = (e.x - me.x).hypot(e.y - me.y);
        if distance > me.pheno.vision || me.kinship.kin(e.kinship) {
            continue;
        }
        let enemy = Threat { id: need.enemy, x: e.x, y: e.y, gap: distance - e.half, half: e.half };
        let key = (Some(s.kinship.id) != prefer, d, s.kinship.id);
        if best.is_none_or(|b| key.0.cmp(&b.0).then(key.1.total_cmp(&b.1)).then(key.2.cmp(&b.2)).is_lt()) {
            best = Some((key.0, key.1, key.2, enemy));
        }
    }
    best.map(|(_, _, id, enemy)| (id, enemy))
}

/// The live plant strictly closer than its sight, within its reach at `taste`, worth most to `me`
/// (`plant_worth`); of equals the nearer, then the first in the plant list.
pub(crate) fn best_plant(grid: &Grid, plants: &[Plant], me: &Me, taste: Taste) -> Option<(f64, f64)> {
    let mut best: Option<(f64, f64, f64, f64, usize)> = None;
    grid.for_each_near(me.x, me.y, me.pheno.vision, |j, px, py| {
        let (dx, dy) = (px - me.x, py - me.y);
        let d2 = dx * dx + dy * dy;
        if !plants[j].alive() || d2 >= me.pheno.vision2 || !taste.admits(py) {
            return;
        }
        let worth = plant_worth(me, px, py, plants[j].portions);
        if best.is_none_or(|b| worth > b.2 || (worth == b.2 && (d2 < b.3 || (d2 == b.3 && j < b.4)))) {
            best = Some((px, py, worth, d2, j));
        }
    });
    best.map(|(px, py, ..)| (px, py))
}

/// The nearest live plant strictly closer than √r2.
#[inline(always)]
pub(crate) fn nearest_plant(grid: &Grid, plants: &[Plant], x: f64, y: f64, r2: f64) -> Option<(f64, f64)> {
    // an eaten one is still in the grid until the tick's end
    grid.nearest(x, y, r2, |j, _, _| plants[j].alive()).map(|(_, px, py)| (px, py))
}

/// The nearest live plant strictly closer than √r2 whose position `keep` accepts.
#[inline(always)]
pub(crate) fn nearest_plant_where(
    grid: &Grid,
    plants: &[Plant],
    x: f64,
    y: f64,
    r2: f64,
    keep: impl Fn(f64, f64) -> bool,
) -> Option<(f64, f64)> {
    grid.nearest(x, y, r2, |j, px, py| plants[j].alive() && keep(px, py)).map(|(_, px, py)| (px, py))
}

/// Take one portion of the nearest plant within the feeding radius.
/// Returns whether it was the last portion (the fifth), and where the plant is.
pub(crate) fn bite_plant(
    grid: &Grid,
    plants: &mut [Plant],
    bitten_this_tick: &mut [bool],
    x: f64,
    y: f64,
    size: f64,
) -> Option<(bool, f64, f64)> {
    debug_assert_eq!(plants.len(), bitten_this_tick.len());
    let r2 = size * size;
    let mut best: Option<(usize, f64)> = None;
    grid.for_each_near(x, y, size, |j, px, py| {
        let (dx, dy) = (x - px, y - py);
        let d2 = dx * dx + dy * dy;
        if plants[j].alive()
            && !bitten_this_tick[j]
            && d2 <= r2
            && best.is_none_or(|(old, distance)| d2 < distance || (d2 == distance && j < old))
        {
            best = Some((j, d2));
        }
    });
    best.and_then(|(j, _)| {
        bitten_this_tick[j] = true;
        plants[j].bite().map(|finished| (finished, plants[j].x, plants[j].y))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Diet gene values, by the variants' order.
    const CARNIVORE: f64 = crate::creature::Diet::Carnivore as usize as f64;
    const SCAVENGER: f64 = crate::creature::Diet::Scavenger as usize as f64;
    use crate::rules::Rules;
    use crate::world::{World, WorldConfig};

    fn dist2(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
        let (dx, dy) = (ax - bx, ay - by);
        dx * dx + dy * dy
    }

    fn min(v: impl Iterator<Item = f64>) -> Option<f64> {
        v.fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.min(x))))
    }

    /// A hunt block's terms with the template's ratio, a caution gene of old (50 = weight 1) and
    /// foreign food allowed: the hunter is hungry.
    fn careful(caution: f64) -> Hunting {
        Hunting {
            ratio: 1.5,
            caution: caution / 50.0,
            taste: Taste { foreign: true, ..Taste::OWN },
            range: f64::INFINITY,
        }
    }

    #[test]
    fn один_укус_берёт_ближайшее_растение_и_разрешает_равенство_по_порядку() {
        let mut plants = vec![Plant::at(1010.0, 1000.0), Plant::at(990.0, 1000.0)];
        let mut grid = Grid::new(GRID_CELL);
        grid.rebuild(&Space::default(), plants.iter().map(|p| (p.x, p.y)));
        let mut eaten_today = vec![false; plants.len()];
        for _ in 0..4 {
            eaten_today.fill(false);
            assert_eq!(
                bite_plant(&grid, &mut plants, &mut eaten_today, 1000.0, 1000.0, 20.0),
                Some((false, 1010.0, 1000.0))
            );
        }
        assert_eq!((plants[0].portions, plants[1].portions), (1, 5));
        eaten_today.fill(false);
        assert_eq!(
            bite_plant(&grid, &mut plants, &mut eaten_today, 1000.0, 1000.0, 20.0),
            Some((true, 1010.0, 1000.0))
        );
        assert!(!plants[0].alive());
        assert_eq!(
            bite_plant(&grid, &mut plants, &mut eaten_today, 1000.0, 1000.0, 20.0),
            Some((false, 990.0, 1000.0))
        );
        assert_eq!(plants[1].portions, 4);
        assert_eq!(bite_plant(&grid, &mut plants, &mut eaten_today, 1000.0, 1000.0, 20.0), None);
    }

    #[test]
    fn одно_растение_не_отдаёт_две_порции_за_тик() {
        let mut plants = [Plant::at(1000.0, 1000.0)];
        let mut grid = Grid::new(GRID_CELL);
        grid.rebuild(&Space::default(), plants.iter().map(|p| (p.x, p.y)));
        let mut bitten = [false];
        assert_eq!(
            bite_plant(&grid, &mut plants, &mut bitten, 1000.0, 1000.0, 40.0).map(|b| b.0),
            Some(false)
        );
        assert_eq!(bite_plant(&grid, &mut plants, &mut bitten, 1000.0, 1000.0, 40.0), None);
        assert_eq!(plants[0].portions, 4);
        bitten.fill(false);
        assert_eq!(
            bite_plant(&grid, &mut plants, &mut bitten, 1000.0, 1000.0, 40.0).map(|b| b.0),
            Some(false)
        );
        assert_eq!(plants[0].portions, 3);
    }

    #[test]
    fn падаль_выбирается_по_порциям_и_дороге_но_не_в_тик_смерти() {
        let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
        w.spawn(
            crate::CreatureGenome::BASE.with(crate::genome::creature::Gene::Diet, CARNIVORE),
            1000.0,
            1000.0,
            Some(40.0),
        );
        let v = &w.creatures[0];
        let me = Me {
            age: 0.0,
            winded: false,
            x: v.x,
            y: v.y,
            energy: v.energy,
            kinship: v.kinship(),
            health_share: 1.0,
            health: v.pheno.size,
            pheno: &v.pheno,
        };
        let mut near = Corpse::from_creature(v, 1);
        near.owner = 2;
        near.x = 1005.0;
        near.remaining = 5.0;
        let mut rich = Corpse::from_creature(v, 1);
        rich.owner = 3;
        rich.x = 1040.0;
        let corpses = [near, rich];
        let mut cgrid = Grid::new(GRID_CELL);
        cgrid.rebuild(&w.space, corpses.iter().map(|c| (c.x, c.y)));
        let food = Grid::new(GRID_CELL);
        let mut view = GridSenses {
            food: &food,
            plants: &[],
            corpse_grid: Some(&cgrid),
            corpses: &corpses,
            now: 1,
            herd: None,
        };
        assert!(view.best_corpse(&me, Taste::OWN).is_none());
        view.now = 2;
        assert_eq!(view.best_corpse(&me, Taste::OWN).unwrap().owner, 3);
    }

    /// Of a fresh and a rotten corpse at equal distance a carnivore picks the fresh one, a
    /// scavenger the rot (each on its own food), and a herbivore sees neither.
    #[test]
    fn падальщик_выбирает_гниль_мясоед_свежее() {
        use crate::genome::creature::Gene;
        let now = 1000;
        for (diet, want) in [(CARNIVORE, Some(1)), (SCAVENGER, Some(2)), (0.0, None), (1.0, Some(1))] {
            let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
            w.spawn(crate::CreatureGenome::BASE.with(Gene::Diet, diet), 1000.0, 1000.0, Some(40.0));
            let v = &w.creatures[0];
            let me = Me {
                age: 0.0,
                winded: false,
                x: v.x,
                y: v.y,
                energy: v.energy,
                kinship: v.kinship(),
                health_share: 1.0,
                health: v.pheno.size,
                pheno: &v.pheno,
            };
            let corpse = |owner: u64, x: f64, born: u64| {
                let mut c = Corpse::from_creature(v, born);
                (c.owner, c.x, c.bottom) = (owner, x, c.y0);
                (c.initial, c.remaining) = (100.0, 60.0);
                c
            };
            let corpses = [corpse(1, 1100.0, now - 10), corpse(2, 900.0, now - 700)];
            use crate::corpse::Stage;
            assert_eq!((corpses[0].stage(now), corpses[1].stage(now)), (Stage::Fresh, Stage::Rot));
            let mut cgrid = Grid::new(GRID_CELL);
            cgrid.rebuild(&w.space, corpses.iter().map(|c| (c.x, c.y)));
            let food = Grid::new(GRID_CELL);
            let view = GridSenses {
                food: &food,
                plants: &[],
                corpse_grid: Some(&cgrid),
                corpses: &corpses,
                now,
                herd: None,
            };
            assert_eq!(view.best_corpse(&me, Taste::OWN).map(|c| c.owner), want, "diet {diet}");
        }
    }

    /// Whether a creature of `diet`, taking foreign food or not, finds through the corpse grid at
    /// tick 1000 a corpse that died at `born` lying `far` of its vision away on its level.
    fn finds_corpse(diet: f64, foreign: bool, born: u64, far: f64) -> bool {
        use crate::genome::creature::Gene;
        let now = 1000;
        let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
        w.spawn(crate::CreatureGenome::BASE.with(Gene::Diet, diet), 1000.0, 1000.0, Some(10.0));
        let v = &w.creatures[0];
        let me = Me {
            age: 0.0,
            winded: false,
            x: v.x,
            y: v.y,
            energy: v.energy,
            kinship: v.kinship(),
            health_share: 1.0,
            health: v.pheno.size,
            pheno: &v.pheno,
        };
        let mut c = Corpse::from_creature(v, born);
        (c.owner, c.x, c.bottom) = (1, 1000.0 + far * v.pheno.vision, c.y0);
        let corpses = [c];
        let mut cgrid = Grid::new(GRID_CELL);
        cgrid.rebuild(&w.space, corpses.iter().map(|c| (c.x, c.y)));
        let food = Grid::new(GRID_CELL);
        let view = GridSenses {
            food: &food,
            plants: &[],
            corpse_grid: Some(&cgrid),
            corpses: &corpses,
            now,
            herd: None,
        };
        view.best_corpse(&me, Taste { foreign, ..Taste::OWN }).is_some()
    }

    /// The scavenger smells corpses from afar; the omnivore finds them by sight. On its own food it
    /// leaves a fresh corpse to the hunters; with its program's «foreign food» setting it goes for
    /// it too.
    #[test]
    fn падальщик_чует_издалека_и_сытым_не_берёт_свежее() {
        let now = 1000;
        for (diet, foreign, born, far, want) in [
            (SCAVENGER, false, now - 700, 1.8, true),
            (1.0, false, now - 700, 1.8, false),
            (SCAVENGER, false, now - 10, 0.5, false),
            (SCAVENGER, true, now - 10, 0.5, true),
        ] {
            assert_eq!(
                finds_corpse(diet, foreign, born, far),
                want,
                "diet {diet}, foreign {foreign}, born {born}"
            );
        }
    }

    /// Each diet smells corpses as far as its `DIET_SMELL` edge says, and no farther, through the
    /// corpse grid itself: the scavenger at 3× its vision, the carnivore at 1.5×, the omnivore at
    /// 1.2×. Hungry, so any corpse will do: even the herbivore goes for fresh meat (10%, not its own
    /// food) — only with its program's «foreign food», and never for rot.
    #[test]
    fn each_diet_smells_as_far_as_its_edge() {
        use crate::config::DIET_SMELL;
        for diet in [1.0, SCAVENGER, CARNIVORE] {
            let smell = DIET_SMELL[diet as usize];
            assert!(finds_corpse(diet, true, 990, smell - 0.05), "diet {diet} smells at {smell}× vision");
            assert!(
                !finds_corpse(diet, true, 990, smell + 0.05),
                "diet {diet} smells no farther than {smell}×"
            );
        }
        assert_eq!(DIET_SMELL, [1.0, 1.2, 3.0, 1.5], "the ranges the user calibrated");
        assert!(finds_corpse(0.0, true, 990, 0.5), "a hungry herbivore takes fresh meat");
        assert!(!finds_corpse(0.0, false, 990, 0.5), "not its own food");
        assert!(!finds_corpse(0.0, true, 300, 0.5), "a herbivore does not go for rot");
    }

    /// Every query of the tick is checked against a brute-force search of all creatures — on the
    /// real positions, sizes and kinship of a live world, not on invented points. It catches a
    /// wrong query radius, a forgotten half of a body, missed kin and a broken grid. The second
    /// world has a cheap size: giants grow there, and the query radius grows with them.
    #[test]
    fn запросы_к_сеткам_совпадают_с_перебором_в_живом_мире() {
        // Food rich enough for bodies past 100. At 120 the biggest reached ~86 once the carnivore
        // and the scavenger smelled farther (2026-09-27); at 180 it is ~190.
        let giants = Rules::default().with("size_power", 1.0).unwrap().with("plant_energy", 180.0).unwrap();
        for (seed, rules) in [(1, Rules::default()), (4, giants)] {
            // the former founders' mix: hunters from the start, for the prey queries to check
            let diets = vec![55.0, 25.0, 10.0, 10.0];
            let mut w = World::new(&WorldConfig { seed, rules, diets, ..Default::default() });
            let mut food = Grid::new(GRID_CELL);
            let mut snapshot = Herd::new();
            let (mut checked, mut threats, mut spared, mut hunters) = (0, 0, 0, 0);
            let (mut hunts, mut guarded, mut seen_prey, mut richer) = (0, 0, 0, 0);
            for tick in 0..1500 {
                w.step();
                if tick % 50 != 0 {
                    continue;
                }
                // some plants were "eaten this tick": the queries must skip them
                let mut plants = w.plants.clone();
                plants.iter_mut().step_by(5).for_each(|p| p.portions = 0);
                food.rebuild(&w.space, plants.iter().map(|p| (p.x, p.y)));

                for v in &w.creatures {
                    let got = nearest_plant(&food, &plants, v.x, v.y, v.pheno.vision2)
                        .map(|(px, py)| dist2(px, py, v.x, v.y));
                    let want = min(plants
                        .iter()
                        .filter(|p| p.alive())
                        .map(|p| dist2(p.x, p.y, v.x, v.y))
                        .filter(|&d2| d2 < v.pheno.vision2));
                    assert_eq!(got, want, "сид {seed}, тик {tick}: ближайшее растение");
                    // the same query limited to a disc beside it
                    let inside = |x: f64, y: f64| dist2(x, y, v.x + 150.0, v.y) <= 200.0 * 200.0;
                    let got = nearest_plant_where(&food, &plants, v.x, v.y, v.pheno.vision2, inside)
                        .map(|(px, py)| dist2(px, py, v.x, v.y));
                    let want = min(plants
                        .iter()
                        .filter(|p| p.alive() && inside(p.x, p.y))
                        .map(|p| dist2(p.x, p.y, v.x, v.y))
                        .filter(|&d2| d2 < v.pheno.vision2));
                    assert_eq!(got, want, "сид {seed}, тик {tick}: ближайшее растение в круге");
                    // the most profitable plant, for some within a band of depth
                    let me = v.me();
                    let reach = if v.id % 2 == 0 { w.space.height * 0.05 } else { f64::INFINITY };
                    let taste = Taste { foreign: false, reach, layer: v.pheno.layer(v.mind.stance.layer) };
                    let got = best_plant(&food, &plants, &me, taste);
                    let want = plants
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| {
                            p.alive() && dist2(p.x, p.y, v.x, v.y) < v.pheno.vision2 && taste.admits(p.y)
                        })
                        .map(|(i, p)| {
                            (i, p, plant_worth(&me, p.x, p.y, p.portions), dist2(p.x, p.y, v.x, v.y))
                        })
                        .max_by(|a, b| a.2.total_cmp(&b.2).then(b.3.total_cmp(&a.3)).then(b.0.cmp(&a.0)))
                        .map(|(_, p, ..)| (p.x, p.y));
                    assert_eq!(got, want, "seed {seed}, tick {tick}: the most profitable plant");
                    let nearest = nearest_plant_where(&food, &plants, v.x, v.y, v.pheno.vision2, |_, y| {
                        taste.admits(y)
                    });
                    richer += (got != nearest) as usize;

                    let expected = plants
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| p.alive())
                        .map(|(i, p)| (i, dist2(p.x, p.y, v.x, v.y)))
                        .filter(|(_, d2)| *d2 <= v.pheno.size2)
                        .min_by(|(ia, da), (ib, db)| da.total_cmp(db).then(ia.cmp(ib)))
                        .map(|(i, _)| i);
                    let mut bitten = plants.clone();
                    let mut eaten_today = vec![false; plants.len()];
                    let last =
                        bite_plant(&food, &mut bitten, &mut eaten_today, v.x, v.y, v.pheno.size).map(|b| b.0);
                    assert_eq!(last, expected.map(|i| plants[i].portions == 1));
                    for (i, (before, after)) in plants.iter().zip(&bitten).enumerate() {
                        assert_eq!(after.portions, before.portions - u8::from(expected == Some(i)));
                        assert_eq!(
                            after.alive(),
                            before.alive() && !(expected == Some(i) && before.portions == 1)
                        );
                    }
                    checked += 1;
                }
                // threats, from a snapshot of the live world as at the start of the phase; each
                // attacker's own program (its hunt blocks) decides whom it can threaten
                snapshot.rebuild(&w.space, &w.creatures);
                let share = crate::config::FLEE_SIGHT_SHARE;
                let lookers =
                    w.creatures.iter().flat_map(|v| [(v, v.pheno.vision), (v, v.pheno.vision * share)]);
                for (v, within) in lookers {
                    let size = v.pheno.size;
                    let got = nearest_threats(&snapshot, v.kinship(), v.x, v.y, size, within);
                    let can_eat_me = |u: &&Creature| {
                        u.pheno.hunts()
                            && u.menace().hunt().is_some_and(|ratio| size <= u.pheno.size / ratio)
                            && dist2(u.x, u.y, v.x, v.y) < (within + u.pheno.half).powi(2)
                    };
                    let gap = |u: &Creature| dist2(u.x, u.y, v.x, v.y).sqrt() - u.pheno.half;
                    let stranger = |u: &&Creature| !v.kinship().kin(u.kinship());
                    let want = min(w.creatures.iter().filter(can_eat_me).filter(stranger).map(gap));
                    assert_eq!(got.0.map(|t| t.gap), want, "сид {seed}, тик {tick}: угроза");
                    let hunting = |u: &&Creature| u.mind.attack.is_some();
                    let want =
                        min(w.creatures.iter().filter(can_eat_me).filter(stranger).filter(hunting).map(gap));
                    assert_eq!(got.1.map(|t| t.gap), want, "seed {seed}, tick {tick}: a hunter");
                    threats += got.0.is_some() as usize;
                    hunters += got.1.is_some() as usize;
                    // kin that would otherwise be a threat: without it the kinship check is empty
                    spared += w
                        .creatures
                        .iter()
                        .filter(can_eat_me)
                        .filter(|u| u.id != v.id && v.kinship().kin(u.kinship()))
                        .count();
                }
                // prey: the same valuation over every creature instead of the grid and the buffers
                let view = GridSenses {
                    food: &food,
                    plants: &plants,
                    corpse_grid: None,
                    corpses: &[],
                    now: tick,
                    herd: Some(&snapshot),
                };
                for v in &w.creatures {
                    let me = Me {
                        age: 0.0,
                        winded: false,
                        x: v.x,
                        y: v.y,
                        energy: v.energy,
                        kinship: v.kinship(),
                        pheno: &v.pheno,
                        health_share: v.health / v.max_health(),
                        health: v.health,
                    };
                    // terms that differ from creature to creature: ratio, foreign food and reach
                    let reach = if v.id % 4 == 1 { w.space.height * 0.1 } else { f64::INFINITY };
                    let hunt = Hunting {
                        ratio: 1.2 + (v.id % 3) as f64 * 0.3,
                        caution: 1.0,
                        taste: Taste {
                            foreign: v.id % 2 == 0,
                            reach,
                            layer: v.pheno.layer(v.mind.stance.layer),
                        },
                        range: v.pheno.vision * if v.id % 5 == 2 { 0.5 } else { 1.0 },
                    };
                    let sees = |u: &Creature| (u.x - v.x).hypot(u.y - v.y) <= hunt.range && u.id != v.id;
                    // prey in sight: the nearest it could eat, by the gap to its body's edge
                    let within = v.pheno.vision * 0.6;
                    let got = nearest_prey(&snapshot, &me, hunt.ratio, hunt.taste, within).map(|t| t.gap);
                    let want = min(w
                        .creatures
                        .iter()
                        .filter(|u| {
                            v.pheno.hunts_now(hunt.taste.foreign)
                                && u.id != v.id
                                && u.pheno.size <= v.pheno.size / hunt.ratio
                                && !v.kinship().kin(u.kinship())
                                && hunt.taste.admits(u.y)
                        })
                        .map(|u| (u.x - v.x).hypot(u.y - v.y) - u.pheno.half)
                        .filter(|&gap| gap < within));
                    assert_eq!(got, want, "seed {seed}, tick {tick}: prey in sight");
                    seen_prey += got.is_some() as usize;
                    let strike = |u: &Creature| u.pheno.strike_on(v.pheno.size);
                    let mut candidates: Vec<usize> = (0..w.creatures.len())
                        .filter(|&j| {
                            let u = &w.creatures[j];
                            v.pheno.hunts_now(hunt.taste.foreign)
                                && sees(u)
                                && u.pheno.size <= v.pheno.size / hunt.ratio
                                && !v.kinship().kin(u.kinship())
                                && hunt.taste.admits(u.y)
                        })
                        .collect();
                    // a full buffer keeps the nearest candidates, ties by id
                    let d2 = |j: usize| (w.creatures[j].x - v.x).powi(2) + (w.creatures[j].y - v.y).powi(2);
                    candidates.sort_by(|&a, &b| {
                        d2(a).total_cmp(&d2(b)).then(w.creatures[a].id.cmp(&w.creatures[b].id))
                    });
                    candidates.truncate(SEEN_PREY);
                    let want = candidates
                        .iter()
                        .map(|&j| {
                            let u = &w.creatures[j];
                            let mut allies = 0.0;
                            if let Some(p) = w.creatures.iter().find(|p| p.id == u.parent)
                                && p.menace().defends
                                && p.kinship().kin(u.kinship())
                                && (p.x - v.x).hypot(p.y - v.y) <= v.pheno.vision
                            {
                                allies += strike(p);
                            }
                            guarded += (allies > 0.0) as usize;
                            Prey::of(&snapshot.seen[j], &me, allies, hunt.caution)
                        })
                        .fold(None, |b: Option<Prey>, p| {
                            if b.is_none_or(|b| p.score > b.score || (p.score == b.score && p.id < b.id)) {
                                Some(p)
                            } else {
                                b
                            }
                        });
                    let got = view.prey(&me, None, None, hunt);
                    // Sums in another order may differ in the last bits: compare scores, not ties.
                    let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1e-12);
                    match (got, want) {
                        (Some(g), Some(e)) => assert!(
                            close(g.score, e.score),
                            "seed {seed}, tick {tick}: prey {} ({}) vs {} ({})",
                            g.id,
                            g.score,
                            e.id,
                            e.score
                        ),
                        (g, e) => {
                            assert_eq!(g.map(|p| p.id), e.map(|p| p.id), "seed {seed}, tick {tick}: prey")
                        }
                    }
                    hunts += got.is_some_and(|p| p.score > 0.0) as usize;
                }
            }
            assert!(checked > 1000, "сид {seed}: проверено всего {checked} запросов — мир вымер?");
            assert!(threats > 100, "сид {seed}: угроз нашлось всего {threats}");
            assert!(spared > 10, "сид {seed}: родни среди угроз всего {spared}");
            assert!(hunters > 10, "seed {seed}: only {hunters} threats were hunting");
            assert!(hunts > 100, "seed {seed}: only {hunts} worthwhile hunts");
            assert!(guarded > 10, "seed {seed}: only {guarded} candidates covered by a parent");
            assert!(seen_prey > 100, "seed {seed}: prey in sight only {seen_prey} times");
            assert!(richer > 10, "seed {seed}: the most profitable plant was the nearest but {richer} times");
            if seed == 4 {
                let biggest = w.creatures.iter().fold(0.0_f64, |m, v| m.max(v.pheno.size));
                assert!(
                    biggest > 100.0,
                    "гиганты не выросли ({biggest:.0}) — вторая часть теста бессмысленна"
                );
            }
        }
    }

    /// A world with a hunter (size 80) at (1000, 1000) holding `energy_share` of its store; `setup`
    /// adds the rest. Returns the hunter's best prey by its valuation on the terms of `hunt`.
    fn best_prey(hunt: Hunting, energy_share: f64, setup: impl Fn(&mut World)) -> Option<Prey> {
        use crate::genome::creature::Gene;
        let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
        let hunter = crate::CreatureGenome::BASE.with(Gene::Size, 80.0).with(Gene::Diet, CARNIVORE);
        w.spawn(hunter, 1000.0, 1000.0, None);
        w.creatures[0].energy = w.creatures[0].pheno.max_energy * energy_share;
        setup(&mut w);
        let mut herd = Herd::new();
        herd.rebuild(&w.space, &w.creatures);
        let food = Grid::new(GRID_CELL);
        let view = GridSenses {
            food: &food,
            plants: &[],
            corpse_grid: None,
            corpses: &[],
            now: 1,
            herd: Some(&herd),
        };
        let v = &w.creatures[0];
        let me = Me {
            age: 0.0,
            winded: false,
            x: v.x,
            y: v.y,
            energy: v.energy,
            kinship: v.kinship(),
            pheno: &v.pheno,
            health_share: 1.0,
            health: v.health,
        };
        view.prey(&me, None, None, hunt)
    }

    /// The reach setting holds the hunt too: a hunter keeping to the upper tenth of the depth with
    /// a reach of 5% leaves prey far below alone, and takes it when it is within reach.
    #[test]
    fn a_hunter_leaves_prey_past_its_layers_reach() {
        use crate::genome::creature::Gene;
        let hunt = |prey_y: f64| {
            let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
            let hunter = crate::CreatureGenome::BASE.with(Gene::Size, 80.0).with(Gene::Diet, CARNIVORE);
            w.spawn(hunter, 1000.0, 300.0, None);
            w.creatures[0].energy = w.creatures[0].pheno.max_energy * 0.3;
            w.spawn(crate::CreatureGenome::BASE.with(Gene::Size, 20.0), 1000.0, prey_y, None);
            w.creatures[1].birth_size = 10.0;
            let mut herd = Herd::new();
            herd.rebuild(&w.space, &w.creatures);
            let food = Grid::new(GRID_CELL);
            let view = GridSenses {
                food: &food,
                plants: &[],
                corpse_grid: None,
                corpses: &[],
                now: 1,
                herd: Some(&herd),
            };
            let v = &w.creatures[0];
            let me = Me {
                age: 0.0,
                winded: false,
                x: v.x,
                y: v.y,
                energy: v.energy,
                kinship: v.kinship(),
                pheno: &v.pheno,
                health_share: 1.0,
                health: v.health,
            };
            // its layer the upper tenth of the depth
            let taste =
                Taste { foreign: true, reach: w.space.height * 0.05, layer: (0.0, w.space.height * 0.1) };
            view.prey(&me, None, None, Hunting { taste, ..careful(50.0) }).map(|p| p.id)
        };
        // the layer ends at 400, the reach at 600; the hunter sees 400 around it
        assert!(hunt(550.0).is_some(), "within reach");
        assert!(hunt(680.0).is_none(), "past its reach, though in sight");
    }

    /// A small creature (size 20, grown from 10: its body is meat) at `x`.
    fn small(w: &mut World, x: f64) -> u64 {
        use crate::genome::creature::Gene;
        let id = w.spawn(crate::CreatureGenome::BASE.with(Gene::Size, 20.0), x, 1000.0, None);
        w.creatures.last_mut().unwrap().birth_size = 10.0;
        id
    }

    /// A parent of `size` at `x` and its growing child (half its adult size) of `child` at
    /// `child_x`: the parent still knows it and covers it.
    fn guarded(w: &mut World, x: f64, size: f64, child_x: f64, child: f64) -> u64 {
        use crate::genome::creature::Gene;
        let parent = w.spawn(crate::CreatureGenome::BASE.with(Gene::Size, size), x, 1000.0, None);
        w.creatures.last_mut().unwrap().mind.stance.spare = 1.0;
        let id = w.spawn(crate::CreatureGenome::BASE.with(Gene::Size, child), child_x, 1000.0, None);
        let v = w.creatures.last_mut().unwrap();
        (v.parent, v.birth_size) = (parent, child / 2.0);
        v.genome = v.genome.with(Gene::Size, child * 2.0);
        id
    }

    #[test]
    fn a_careful_hunter_leaves_a_guarded_prey_for_a_lone_one() {
        let (near, far) = (std::cell::Cell::new(0), std::cell::Cell::new(0));
        let setup = |w: &mut World| {
            near.set(guarded(w, 1100.0, 120.0, 1060.0, 20.0)); // closer, but its parent is beside it
            far.set(small(w, 925.0)); // a loner a little farther away
        };
        let careless = best_prey(careful(0.0), 0.3, setup).unwrap();
        let careful = best_prey(careful(50.0), 0.3, setup).unwrap();
        assert_eq!(careless.id, near.get(), "a careless hunter takes the nearest");
        assert_eq!(careful.id, far.get(), "a careful hunter took the guarded one");
        assert!(careful.score > 0.0);
    }

    #[test]
    fn a_full_hunter_gains_nothing_and_a_hungry_one_something() {
        let setup = |w: &mut World| {
            small(w, 1060.0);
        };
        assert_eq!(best_prey(careful(50.0), 1.0, setup).unwrap().score, 0.0);
        assert!(best_prey(careful(50.0), 0.3, setup).unwrap().score > 0.0);
    }

    #[test]
    fn a_hunt_that_looks_deadly_is_worth_nothing_unless_careless() {
        // Prey as big as the hunter, that fights back, with a bigger parent beside it.
        let setup = |w: &mut World| {
            guarded(w, 1150.0, 120.0, 1080.0, 80.0);
        };
        // a hunt block that takes prey as big as the hunter
        assert_eq!(best_prey(Hunting { ratio: 1.0, ..careful(50.0) }, 0.3, setup).unwrap().score, 0.0);
        assert!(best_prey(Hunting { ratio: 1.0, ..careful(0.0) }, 0.3, setup).unwrap().score > 0.0);
    }

    #[test]
    fn a_parent_in_sight_covers_its_growing_child_only_while_it_knows_it() {
        use crate::genome::creature::Gene;
        let with_parent = |care: f64, defends: bool| {
            move |w: &mut World| {
                let parent =
                    w.spawn(crate::CreatureGenome::BASE.with(Gene::Size, 60.0), 1100.0, 1000.0, None);
                // the old `care` gene was half of how long it knows its children
                let p = w.creatures.last_mut().unwrap();
                p.mind.stance.spare = (care / 50.0).min(1.0);
                if !defends {
                    use crate::creature::{Action, Block, Program, Programs};
                    p.programs = Programs::both(Program::of(&[Block::does(Action::Wander)]));
                }
                small(w, 1060.0);
                let child = w.creatures.last_mut().unwrap();
                child.parent = parent;
                child.genome = child.genome.with(Gene::Size, 40.0); // half grown
            }
        };
        let alone = best_prey(careful(50.0), 0.3, |w: &mut World| {
            small(w, 1060.0);
        })
        .unwrap();
        let covered = best_prey(careful(50.0), 0.3, with_parent(50.0, true)).unwrap();
        let forgotten = best_prey(careful(50.0), 0.3, with_parent(10.0, true)).unwrap();
        let careless = best_prey(careful(50.0), 0.3, with_parent(50.0, false)).unwrap();
        assert!(covered.score < alone.score, "a parent in sight did not count");
        assert_eq!(forgotten.score, alone.score, "a parent that forgot its child still covers it");
        assert_eq!(careless.score, alone.score, "a parent whose program does not defend covers it");
    }

    /// A full candidate buffer keeps the nearest prey: a crowd in the grid rows above the hunter
    /// (scanned first) does not hide the prey next to it.
    #[test]
    fn a_crowd_higher_up_does_not_hide_the_nearest_prey() {
        let near = std::cell::Cell::new(0);
        let best = best_prey(careful(50.0), 0.3, |w: &mut World| {
            // a row of small loners in the grid row above the hunter's, 300..360 away
            let small_genome = crate::CreatureGenome::BASE.with(crate::genome::creature::Gene::Size, 20.0);
            for i in 0..SEEN_PREY {
                w.spawn(small_genome, 810.0 + 6.0 * i as f64, 700.0, None);
            }
            near.set(small(w, 1030.0));
        })
        .unwrap();
        assert_eq!(best.id, near.get(), "the nearest prey was never looked at");
    }

    /// Whom a stranger fears: anyone whose program hunts bodies of its size (the threat), and of
    /// them the ones hunting somebody now (the hunter). A carnivore whose program has no hunt block
    /// threatens no one. The herbivore digests a little fresh meat (10%): before its first move its
    /// hunt block makes it feared like any hunter; a diet with no fresh meat eats no one.
    #[test]
    fn a_threat_is_whoever_could_hunt_it_a_hunter_whoever_does() {
        use crate::creature::{Action, Block, Program};
        use crate::genome::creature::Gene;
        for (diet, hunts, hunting, threat, hunter) in [
            (SCAVENGER, true, false, true, false),
            (CARNIVORE, true, true, true, true),
            (CARNIVORE, false, true, false, false),
            (0.0, true, true, true, true),
            (-1.0, true, true, false, false),
        ] {
            let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
            // -1: a herbivore that digests no fresh meat
            let diet = if diet < 0.0 {
                w.set_rules(w.rules.with("herbivore_meat", 0.0).unwrap());
                0.0
            } else {
                diet
            };
            w.spawn(
                crate::CreatureGenome::BASE.with(Gene::Size, 80.0).with(Gene::Diet, diet),
                1000.0,
                1000.0,
                None,
            );
            if !hunts {
                w.creatures[0].programs = [Program::of(&[Block::does(Action::Wander)]); 2].into();
            }
            // 60 from the edge of the big body
            w.spawn(crate::CreatureGenome::BASE.with(Gene::Size, 15.0), 1100.0, 1000.0, None);
            if hunting {
                w.creatures[0].mind.attack = Some(999);
            }
            let mut herd = Herd::new();
            herd.rebuild(&w.space, &w.creatures);
            let v = &w.creatures[1];
            let (any, hunts_now) = nearest_threats(&herd, v.kinship(), v.x, v.y, v.pheno.size, 100.0);
            let case = format!("diet {diet}, hunt block {hunts}, hunting {hunting}");
            assert_eq!((any.is_some(), hunts_now.is_some()), (threat, hunter), "{case}");
            let (none, _) = nearest_threats(&herd, v.kinship(), v.x, v.y, v.pheno.size, 50.0);
            assert!(none.is_none(), "{case}: farther than asked");
        }
    }
}
