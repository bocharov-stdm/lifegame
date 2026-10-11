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
//! step), so the decisions run in parallel (`par.rs`; CLAUDE.md «Performance», «Threads»).

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
    /// `within` — by the herd's snapshot at the start of the phase. Asked only by the default
    /// `threats_near`: the world's senses answer that themselves.
    fn nearest_threat(&self, _me: &Me, _within: f64) -> Option<Threat> {
        None
    }

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
    pub corpse_grid: &'a Grid,
    pub corpses: &'a [Corpse],
    pub now: u64,
    /// The snapshot of the herd.
    pub herd: &'a Herd,
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
        self.corpse_grid.for_each_near(me.x, me.y, me.pheno.smell, |i, cx, cy| {
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
        let herd = self.herd;
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
        let herd = self.herd;
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
            // A parent in sight that still knows it and whose program defends its children — unless
            // it knows the hunter too: kin strike no kin (`combat`), nor defend a child from them.
            let mut allies = 0.0;
            if let Ok(k) = herd.seen.binary_search_by_key(&s.kinship.parent, |p| p.kinship.id) {
                let p = &herd.seen[k];
                if p.defends
                    && p.kinship.kin(s.kinship)
                    && !me.kinship.kin(p.kinship)
                    && (p.x - me.x).hypot(p.y - me.y) <= me.pheno.vision
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
        nearest_prey(self.herd, me, ratio, taste, within)
    }

    fn child_in_need(
        &self,
        me: &Me,
        within: f64,
        tick: u64,
        window: u64,
        prefer: Option<u64>,
    ) -> Option<(u64, Threat)> {
        child_in_need(self.herd, me, within, tick, window, prefer)
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
    fn threats_near(&self, me: &Me, within: f64) -> (Option<Threat>, Option<Threat>) {
        nearest_threats(self.herd, me.kinship, me.x, me.y, me.pheno.size, within)
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
mod tests;
