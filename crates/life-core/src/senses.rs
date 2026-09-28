//! Чувства: что существо может узнать о мире.
//!
//! Существа не видят сетку соседей: они спрашивают «где ближайшее растение»,
//! «кто рядом может меня съесть» — через трейт. Мир отвечает по сеткам
//! (`GridSenses` ниже, строится на каждое существо), тесты — обычными
//! замыканиями (`senses_from`) или слепотой (`Blind`).
//!
//! Новое чувство — метод трейта, функция-запрос внизу и её сверка с перебором
//! в тесте этого модуля.
//!
//! Свой вид существа видят только по снимку на начало фазы (`Herd`). В своей
//! фазе они двигаются: копии координат в сетке устарели бы, а чтение живых
//! позиций сделало бы исход зависимым от порядка обхода. По снимку все видят
//! соседей там, где те стояли в начале тика (отставание — не больше шага), и
//! параллельный тик сможет решать за всех одновременно (CLAUDE.md,
//! «Neighbour search»).

use crate::config::GRID_CELL;
use crate::corpse::Corpse;
use crate::creature::{Creature, Kinship, Me};
use crate::grid::Grid;
use crate::kin_grace::Grace;
use crate::plant::Plant;
use crate::space::Space;

/// What a creature takes for food this tick, as its program's settings set it (`Stance`): the
/// other niche's food too, and how far past its layer (y; infinite: anywhere).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Taste {
    pub foreign: bool,
    pub reach: f64,
}

impl Taste {
    /// Only its own food, anywhere: what it reports to its neighbours.
    pub const OWN: Taste = Taste { foreign: false, reach: f64::INFINITY };
}

/// The terms of a hunt block (`Action::Hunt`): prey this many times smaller, the weight of the
/// strikes it expects (1: the old base caution), and what it takes for food.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hunting {
    pub ratio: f64,
    pub caution: f64,
    pub taste: Taste,
}

/// Чувства существа.
pub trait Senses {
    fn visible_enemy(&self, _me: &Me, _id: u64) -> Option<Threat> {
        None
    }
    /// Ближайшее живое растение строго ближе √r2.
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

    /// Ближайший чужак (не родня), который может меня съесть и до края тела
    /// которого меньше `within`, — по снимку стада на начало фазы.
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
}

/// Оценка добычи только по лично видимому существу.
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

/// How many flocks and prey candidates a hunter keeps in mind at once. The buffers live on the
/// stack. A full prey buffer keeps the nearest candidates (ties by id) and the current target, so
/// what the hunter weighs does not depend on the grid's scan order. Flocks past the first
/// `SEEN_FLOCKS` in sight are not summed (rarely reached: at most 28 were seen in a ×1 world).
const SEEN_FLOCKS: usize = 32;
const SEEN_PREY: usize = 64;

/// Опасный чужак, каким его видит существо.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Threat {
    pub id: u64,
    /// Центр его тела.
    pub x: f64,
    pub y: f64,
    /// Расстояние до края его тела; внутри тела — меньше нуля.
    pub gap: f64,
    /// Радиус его тела: заведомо сильнейшему в ответ не бьют, от него бегут.
    pub half: f64,
}

/// Мир глазами существа: растения по сетке тика, сородичи по снимку стада.
///
/// Запросы и методы чувств помечены `#[inline(always)]`: без этого компилятор
/// не встраивал поиск растения в ход существа, и мир ×100 шёл на 8%
/// медленнее, чем с прежними замыканиями (замер против версии до чувств).
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
                || !me.pheno.within_reach(cy, taste.reach)
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
        if me.kinship.kin(s.kinship)
            || me.flock == s.flock
            || herd.grace.contains(me.flock, s.flock, herd.tick)
            || distance > me.pheno.vision
        {
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
        // One look around: the strikes of every visible flock but one's own, and the candidates.
        // Loners have a label each and cover nobody, so only members of real flocks are summed.
        let mut flocks = [(0_u64, 0.0_f64); SEEN_FLOCKS];
        let mut n_flocks = 0;
        // (kept first, distance², id, index): the order in which a full buffer keeps candidates
        let mut candidates = [(false, 0.0_f64, 0_u64, 0_usize); SEEN_PREY];
        let mut n_candidates = 0;
        herd.grid.for_each_near(me.x, me.y, me.pheno.vision, |j, _, _| {
            let s = &herd.seen[j];
            let d2 = (s.x - me.x).powi(2) + (s.y - me.y).powi(2);
            if d2.sqrt() > me.pheno.vision || s.kinship.id == me.kinship.id {
                return;
            }
            if s.grouped && s.flock != me.flock {
                match flocks[..n_flocks].iter_mut().find(|f| f.0 == s.flock) {
                    Some(f) => f.1 += s.strike_on(me),
                    None if n_flocks < SEEN_FLOCKS => {
                        flocks[n_flocks] = (s.flock, s.strike_on(me));
                        n_flocks += 1;
                    }
                    None => {}
                }
            }
            if (me.flock != 0 && me.flock == s.flock)
                || s.half * 2.0 > max_size
                || me.kinship.kin(s.kinship)
                || herd.grace.contains(me.flock, s.flock, herd.tick)
                || Some(s.kinship.id) == avoid
                // past its layer's reach, as plants and corpses (`Action::Reach`)
                || !me.pheno.within_reach(s.y, hunt.taste.reach)
            {
                return;
            }
            let key = (Some(s.kinship.id) != previous, d2, s.kinship.id, j);
            if n_candidates < SEEN_PREY {
                candidates[n_candidates] = key;
                n_candidates += 1;
            } else {
                let rank = |c: &(bool, f64, u64, usize)| (c.0, c.1, c.2);
                let worst = (0..SEEN_PREY)
                    .max_by(|&a, &b| {
                        let (x, y) = (rank(&candidates[a]), rank(&candidates[b]));
                        x.0.cmp(&y.0).then(x.1.total_cmp(&y.1)).then(x.2.cmp(&y.2))
                    })
                    .unwrap();
                let (x, y) = (rank(&key), rank(&candidates[worst]));
                if x.0.cmp(&y.0).then(x.1.total_cmp(&y.1)).then(x.2.cmp(&y.2)).is_lt() {
                    candidates[worst] = key;
                }
            }
        });
        let mut best: Option<Prey> = None;
        for &(_, _, _, j) in &candidates[..n_candidates] {
            let s = &herd.seen[j];
            // Its flockmates in sight, and a parent in sight that still knows it and would cover it.
            let mut allies = if s.grouped {
                flocks[..n_flocks].iter().find(|f| f.0 == s.flock).map_or(0.0, |f| f.1) - s.strike_on(me)
            } else {
                0.0
            };
            if let Ok(k) = herd.seen.binary_search_by_key(&s.kinship.parent, |p| p.kinship.id) {
                let p = &herd.seen[k];
                if p.flock != s.flock
                    && p.kinship.kin(s.kinship)
                    && (p.x - me.x).hypot(p.y - me.y) <= me.pheno.vision
                {
                    allies += p.strike_on(me);
                }
            }
            let p = Prey::of(s, me, allies.max(0.0), hunt.caution);
            let kept = |b: &Prey| Some(b.id) == previous && b.score > 0.0;
            if best.is_none_or(|b| {
                !kept(&b) && (kept(&p) || p.score > b.score || (p.score == b.score && p.id < b.id))
            }) {
                best = Some(p);
            }
        }
        best
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
            Some(herd) => nearest_threats(herd, me.kinship, me.flock, me.x, me.y, me.pheno.size, within),
            None => (None, None),
        }
    }
}

/// Чувства для тестов: растение из замыкания `senses_from(|x, y, r2| ..)`,
/// угроза — заданная (`with_threat`; видна, если ближе запрошенного) или никакой.
pub struct FnSenses<F> {
    plant: F,
    threat: Option<Threat>,
}

/// Существу: ближайшее растение; угроз нет.
pub fn senses_from<F>(plant: F) -> FnSenses<F>
where
    F: Fn(f64, f64, f64) -> Option<(f64, f64)>,
{
    FnSenses { plant, threat: None }
}

impl<F> FnSenses<F> {
    /// Те же чувства, но рядом опасный чужак.
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

/// Ничего не видит.
pub struct Blind;

impl Senses for Blind {
    fn nearest_plant(&self, _: f64, _: f64, _: f64) -> Option<(f64, f64)> {
        None
    }

    fn nearest_threat(&self, _: &Me, _: f64) -> Option<Threat> {
        None
    }
}

// ── снимок стада ────────────────────────────────────────────────────────────

/// Существо в снимке стада: что о нём видно другим.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Seen {
    x: f64,
    y: f64,
    /// Радиус тела: до чужака меряют расстояние до края тела, а не до центра.
    half: f64,
    /// Самое крупное тело, какое он может съесть; 0 — он не ест свежего мяса и никому не угроза.
    eats_up_to: f64,
    health: f64,
    nutrition: f64,
    /// Its melee strike: what a hunter expects back from it or from it as an ally.
    strike: f64,
    /// Its program's defence (`Program::defence`): it strikes back an enemy with a smaller radius
    /// than this (0: nobody), for this share of the strikes that kill it (down to its block's
    /// health threshold). A hunter weighs what the prey will do, as the prey fears the hunter's
    /// hunt block.
    fights_below: f64,
    fights_share: f64,
    kinship: Kinship,
    flock: u64,
    /// In a flock of two or more (it has a circle): its flockmates may cover it.
    grouped: bool,
    /// It chose a target on its last move: it is hunting (or fighting) someone.
    hunting: bool,
}

impl Seen {
    /// Its strike on `me`: harder when it is the bigger (`phenotype::melee_damage`).
    #[inline]
    fn strike_on(&self, me: &Me) -> f64 {
        crate::creature::melee_damage(self.strike, self.half * 2.0, me.pheno.size, me.pheno.melee_size_power)
    }
}

/// Снимок всех существ на начало фазы: мелкие нужны как добыча, крупные — как угрозы.
/// Запросы используют сетку; буферы живут между тиками.
#[derive(Clone, Debug)]
pub(crate) struct Herd {
    grid: Grid,
    seen: Vec<Seen>,
    /// Самое крупное тело, какое может съесть хоть кто-то. Кто крупнее, тому
    /// бояться некого, и в сетку он не смотрит.
    max_eats: f64,
    /// Самый большой радиус тела в снимке: на него шире запрос.
    max_half: f64,
    grace: Grace,
    tick: u64,
}

impl Herd {
    pub fn new() -> Self {
        Herd {
            grid: Grid::new(GRID_CELL),
            seen: Vec::new(),
            max_eats: 0.0,
            max_half: 0.0,
            grace: Grace::default(),
            tick: 0,
        }
    }

    /// A snapshot of the creatures as they stand now. At the start of the phase all are alive:
    /// the dead are swept at the end of the previous one. The ones looking into the snapshot are
    /// the ones in it: children are born after the moves. Whom one may eat is the most permissive
    /// hunt of the program it lives by now (`Program::hunt_ratio`).
    #[cfg(test)]
    pub fn rebuild(&mut self, space: &Space, creatures: &[Creature]) {
        self.rebuild_with_grace(space, creatures, &Grace::default(), 0);
    }

    pub fn rebuild_with_grace(&mut self, space: &Space, creatures: &[Creature], grace: &Grace, tick: u64) {
        debug_assert!(creatures.iter().all(|v| v.alive), "в снимке стада мёртвые");
        self.grace.clone_from(grace);
        self.tick = tick;
        self.seen.clear();
        self.seen.extend(creatures.iter().map(|v| {
            let program = v.program();
            let (fights_below, fights_share) =
                program.defence().map_or((0.0, 0.0), |(ratio, health)| (v.pheno.half * ratio, 1.0 - health));
            Seen {
                x: v.x,
                y: v.y,
                half: v.pheno.half,
                eats_up_to: match program.hunt_ratio() {
                    Some(ratio) if v.pheno.hunts() => v.pheno.size / ratio,
                    _ => 0.0,
                },
                health: v.health,
                nutrition: crate::corpse::meat(v),
                strike: v.pheno.strike(),
                fights_below,
                fights_share,
                kinship: v.kinship(),
                flock: v.flock,
                grouped: v.circle.is_some(),
                hunting: v.mind.attack.is_some(),
            }
        }));
        self.grid.rebuild(space, self.seen.iter().map(|s| (s.x, s.y)));
        let (mut max_eats, mut max_half) = (0.0_f64, 0.0_f64);
        for s in &self.seen {
            max_eats = max_eats.max(s.eats_up_to);
            max_half = max_half.max(s.half);
        }
        (self.max_eats, self.max_half) = (max_eats, max_half);
    }
}

// ── запросы к сеткам ────────────────────────────────────────────────────────
// Вынесены из тика, чтобы тест мог сверить их с честным перебором в живом мире:
// ошибка в радиусе запроса не роняет ничего, а тихо меняет баланс — существа
// перестают замечать соседей под носом.

/// The nearest strangers (not family of `who`) from the snapshot, measured to the edge of their
/// bodies, that could eat a body of `size` and are closer than `within`: the nearest of all, and
/// the nearest hunting somebody (it chose a target on its last move). How near a calm one may come
/// is the program's choice (`Cond::ThreatNear`, `Cond::HunterNear`).
#[inline(always)]
pub(crate) fn nearest_threats(
    herd: &Herd,
    who: Kinship,
    flock: u64,
    x: f64,
    y: f64,
    size: f64,
    within: f64,
) -> (Option<Threat>, Option<Threat>) {
    if size > herd.max_eats {
        return (None, None); // nobody in the world can eat such a body
    }
    let (mut best, mut hunter): (Option<Threat>, Option<Threat>) = (None, None);
    herd.grid.for_each_near(x, y, within + herd.max_half, |j, sx, sy| {
        let s = &herd.seen[j];
        if size > s.eats_up_to
            || who.kin(s.kinship)
            || (flock != 0 && flock == s.flock)
            || herd.grace.contains(flock, s.flock, herd.tick)
        {
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

/// Ближайшее живое растение строго ближе √r2.
#[inline(always)]
pub(crate) fn nearest_plant(grid: &Grid, plants: &[Plant], x: f64, y: f64, r2: f64) -> Option<(f64, f64)> {
    let mut best: Option<(f64, f64, f64)> = None;
    grid.for_each_near(x, y, r2.sqrt(), |j, px, py| {
        if !plants[j].alive() {
            return; // съедено раньше в этом же тике
        }
        let (dx, dy) = (px - x, py - y);
        let d2 = dx * dx + dy * dy;
        if d2 < best.map_or(r2, |b| b.2) {
            best = Some((px, py, d2));
        }
    });
    best.map(|(px, py, _)| (px, py))
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
    let mut best: Option<(f64, f64, f64)> = None;
    grid.for_each_near(x, y, r2.sqrt(), |j, px, py| {
        if !plants[j].alive() || !keep(px, py) {
            return;
        }
        let (dx, dy) = (px - x, py - y);
        let d2 = dx * dx + dy * dy;
        if d2 < best.map_or(r2, |b| b.2) {
            best = Some((px, py, d2));
        }
    });
    best.map(|(px, py, _)| (px, py))
}

/// Взять одну порцию ближайшего растения в радиусе питания.
/// Возвращает, последняя ли это порция (пятая), и где растение.
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
    use crate::flock::Circle;
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
        Hunting { ratio: 1.5, caution: caution / 50.0, taste: Taste { foreign: true, reach: f64::INFINITY } }
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
            x: v.x,
            y: v.y,
            energy: v.energy,
            kinship: v.kinship(),
            flock: v.flock,
            circle: v.circle,
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
                x: v.x,
                y: v.y,
                energy: v.energy,
                kinship: v.kinship(),
                flock: v.flock,
                circle: None,
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
            x: v.x,
            y: v.y,
            energy: v.energy,
            kinship: v.kinship(),
            flock: v.flock,
            circle: None,
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
        view.best_corpse(&me, Taste { foreign, reach: f64::INFINITY }).is_some()
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
    /// corpse grid itself: the scavenger at 3× its vision, the carnivore at 1.5×, the omnivore by
    /// sight; the herbivore, eating no meat, never goes for one. Hungry, so any corpse will do.
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
        assert_eq!(DIET_SMELL, [1.0, 1.0, 3.0, 1.5], "the ranges the niches were measured with");
        assert!(!finds_corpse(0.0, true, 990, 0.5), "a herbivore does not go for meat");
    }

    #[test]
    fn разделившиеся_стаи_не_видят_друг_друга_целью_охоты_или_угрозой_до_срока() {
        let mut world = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
        world.spawn(
            crate::CreatureGenome::BASE
                .with(crate::genome::creature::Gene::Size, 80.0)
                .with(crate::genome::creature::Gene::Diet, CARNIVORE),
            1000.0,
            1000.0,
            None,
        );
        world.spawn(
            crate::CreatureGenome::BASE.with(crate::genome::creature::Gene::Size, 15.0),
            1050.0,
            1000.0,
            None,
        );
        let mut grace = Grace::default();
        grace.register(world.creatures[0].flock, world.creatures[1].flock, 0);
        let mut herd = Herd::new();
        let food = Grid::new(GRID_CELL);
        let predator = &world.creatures[0];
        let prey = &world.creatures[1];
        let hunter = Me {
            x: predator.x,
            y: predator.y,
            energy: predator.energy,
            kinship: predator.kinship(),
            flock: predator.flock,
            circle: None,
            pheno: &predator.pheno,
            health_share: 1.0,
            health: predator.pheno.size,
        };
        let hunted = Me {
            x: prey.x,
            y: prey.y,
            energy: prey.energy,
            kinship: prey.kinship(),
            flock: prey.flock,
            circle: None,
            pheno: &prey.pheno,
            health_share: 1.0,
            health: prey.pheno.size,
        };
        for (tick, safe) in [(600, true), (601, false)] {
            herd.rebuild_with_grace(&world.space, &world.creatures, &grace, tick);
            let view = GridSenses {
                food: &food,
                plants: &[],
                corpse_grid: None,
                corpses: &[],
                now: tick,
                herd: Some(&herd),
            };
            assert_eq!(view.prey(&hunter, None, None, careful(50.0)).is_none(), safe);
            assert_eq!(view.nearest_threat(&hunted, hunted.pheno.vision).is_none(), safe);
            assert_eq!(view.visible_enemy(&hunter, prey.id).is_none(), safe);
        }
    }

    /// Каждый запрос тика сверяется с перебором всех существ — на настоящих
    /// позициях, размерах и родстве живого мира, а не на выдуманных точках.
    /// Ловит неверный радиус запроса, забытую половину тела, пропущенную родню
    /// и сломанную сетку. Второй мир — с дешёвым размером: там вырастают
    /// гиганты, и радиус запроса растёт с ними.
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
            let (mut hunts, mut guarded, mut crowded) = (0, 0, 0);
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
                    // the same query limited to a circle: the creature's own, or one beside it
                    let circle = v.circle.unwrap_or(Circle { x: v.x + 150.0, y: v.y, radius: 200.0 });
                    let got = nearest_plant_where(&food, &plants, v.x, v.y, v.pheno.vision2, |x, y| {
                        circle.holds(x, y, v.pheno.half)
                    })
                    .map(|(px, py)| dist2(px, py, v.x, v.y));
                    let want = min(plants
                        .iter()
                        .filter(|p| p.alive() && circle.holds(p.x, p.y, v.pheno.half))
                        .map(|p| dist2(p.x, p.y, v.x, v.y))
                        .filter(|&d2| d2 < v.pheno.vision2));
                    assert_eq!(got, want, "сид {seed}, тик {tick}: ближайшее растение в круге");

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
                    let got = nearest_threats(&snapshot, v.kinship(), v.flock, v.x, v.y, size, within);
                    let can_eat_me = |u: &&Creature| {
                        u.pheno.hunts()
                            && u.program().hunt_ratio().is_some_and(|ratio| size <= u.pheno.size / ratio)
                            && dist2(u.x, u.y, v.x, v.y) < (within + u.pheno.half).powi(2)
                    };
                    let gap = |u: &Creature| dist2(u.x, u.y, v.x, v.y).sqrt() - u.pheno.half;
                    let stranger = |u: &&Creature| !v.kinship().kin(u.kinship()) && v.flock != u.flock;
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
                        x: v.x,
                        y: v.y,
                        energy: v.energy,
                        kinship: v.kinship(),
                        flock: v.flock,
                        circle: v.circle,
                        pheno: &v.pheno,
                        health_share: v.health / v.max_health(),
                        health: v.health,
                    };
                    // terms that differ from creature to creature: ratio, foreign food and reach
                    let reach = if v.id % 4 == 1 { w.space.height * 0.1 } else { f64::INFINITY };
                    let hunt = Hunting {
                        ratio: 1.2 + (v.id % 3) as f64 * 0.3,
                        caution: 1.0,
                        taste: Taste { foreign: v.id % 2 == 0, reach },
                    };
                    let sees = |u: &Creature| (u.x - v.x).hypot(u.y - v.y) <= v.pheno.vision && u.id != v.id;
                    let strike = |u: &Creature| u.pheno.strike_on(v.pheno.size);
                    let mut candidates: Vec<usize> = (0..w.creatures.len())
                        .filter(|&j| {
                            let u = &w.creatures[j];
                            v.pheno.hunts_now(hunt.taste.foreign)
                                && sees(u)
                                && !(v.flock != 0 && v.flock == u.flock)
                                && u.pheno.size <= v.pheno.size / hunt.ratio
                                && !v.kinship().kin(u.kinship())
                                && v.pheno.within_reach(u.y, hunt.taste.reach)
                        })
                        .collect();
                    let flocks: std::collections::BTreeSet<u64> = w
                        .creatures
                        .iter()
                        .filter(|u| sees(u) && u.circle.is_some() && u.flock != v.flock)
                        .map(|u| u.flock)
                        .collect();
                    if flocks.len() > SEEN_FLOCKS {
                        crowded += 1;
                        continue;
                    }
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
                            let mut allies: f64 = w
                                .creatures
                                .iter()
                                .filter(|a| {
                                    u.circle.is_some()
                                        && a.circle.is_some()
                                        && sees(a)
                                        && a.flock == u.flock
                                        && a.flock != v.flock
                                        && a.id != u.id
                                })
                                .map(strike)
                                .sum();
                            if let Some(p) = w.creatures.iter().find(|p| p.id == u.parent)
                                && p.flock != u.flock
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
            assert!(guarded > 100, "seed {seed}: only {guarded} guarded candidates");
            assert!(
                crowded * 20 < checked,
                "seed {seed}: {crowded} hunters saw more flocks than the buffer holds"
            );
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
            x: v.x,
            y: v.y,
            energy: v.energy,
            kinship: v.kinship(),
            flock: v.flock,
            circle: None,
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
            let hunter = crate::CreatureGenome::BASE
                .with(Gene::Size, 80.0)
                .with(Gene::Diet, CARNIVORE)
                .with(Gene::MinY, 0.0)
                .with(Gene::MaxY, 10.0);
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
                x: v.x,
                y: v.y,
                energy: v.energy,
                kinship: v.kinship(),
                flock: v.flock,
                circle: None,
                pheno: &v.pheno,
                health_share: 1.0,
                health: v.health,
            };
            let reach = w.space.height * 0.05;
            view.prey(&me, None, None, Hunting { taste: Taste { foreign: true, reach }, ..careful(50.0) })
                .map(|p| p.id)
        };
        // the layer ends at 400, the reach at 600; the hunter sees 400 around it
        assert!(hunt(550.0).is_some(), "within reach");
        assert!(hunt(680.0).is_none(), "past its reach, though in sight");
    }

    /// A small creature (size 20, grown from 10: its body is meat) at `x`; `flock` with a circle
    /// makes it a flock member.
    fn small(w: &mut World, x: f64, flock: Option<u64>) -> u64 {
        use crate::genome::creature::Gene;
        let id = w.spawn(crate::CreatureGenome::BASE.with(Gene::Size, 20.0), x, 1000.0, None);
        w.creatures.last_mut().unwrap().birth_size = 10.0;
        if let Some(flock) = flock {
            let v = w.creatures.last_mut().unwrap();
            v.flock = flock;
            v.circle = Some(Circle { x, y: 1000.0, radius: 150.0 });
        }
        id
    }

    #[test]
    fn a_careful_hunter_leaves_a_guarded_prey_for_a_lone_one() {
        let setup = |w: &mut World| {
            small(w, 1060.0, Some(500)); // closer, but with three flockmates beside it
            for dx in [80.0, 100.0, 120.0] {
                small(w, 1000.0 + dx, Some(500));
            }
            small(w, 925.0, None); // a loner a little farther away
        };
        let careless = best_prey(careful(0.0), 0.3, setup).unwrap();
        let careful = best_prey(careful(50.0), 0.3, setup).unwrap();
        assert_eq!(careless.id, 2, "a careless hunter takes the nearest");
        assert_eq!(careful.id, 6, "a careful hunter took the guarded one");
        assert!(careful.score > 0.0);
    }

    #[test]
    fn a_full_hunter_gains_nothing_and_a_hungry_one_something() {
        let setup = |w: &mut World| {
            small(w, 1060.0, None);
        };
        assert_eq!(best_prey(careful(50.0), 1.0, setup).unwrap().score, 0.0);
        assert!(best_prey(careful(50.0), 0.3, setup).unwrap().score > 0.0);
    }

    #[test]
    fn a_hunt_that_looks_deadly_is_worth_nothing_unless_careless() {
        use crate::genome::creature::Gene;
        // The prey's flock: five creatures as big as the hunter; the prey is one of them.
        let setup = |w: &mut World| {
            for dx in [60.0, 70.0, 80.0, 90.0, 100.0] {
                w.spawn(crate::CreatureGenome::BASE.with(Gene::Size, 80.0), 1000.0 + dx, 1000.0, None);
                let v = w.creatures.last_mut().unwrap();
                v.flock = 500;
                v.circle = Some(Circle { x: 1080.0, y: 1000.0, radius: 150.0 });
            }
        };
        // a hunt block that takes prey as big as the hunter
        assert_eq!(best_prey(Hunting { ratio: 1.0, ..careful(50.0) }, 0.3, setup).unwrap().score, 0.0);
        assert!(best_prey(Hunting { ratio: 1.0, ..careful(0.0) }, 0.3, setup).unwrap().score > 0.0);
    }

    #[test]
    fn a_parent_in_sight_covers_its_growing_child_only_while_it_knows_it() {
        use crate::genome::creature::Gene;
        let with_parent = |care: f64| {
            move |w: &mut World| {
                let parent = w.spawn(
                    crate::CreatureGenome::BASE.with(Gene::Size, 60.0).with(Gene::Care, care),
                    1100.0,
                    1000.0,
                    None,
                );
                small(w, 1060.0, None);
                let child = w.creatures.last_mut().unwrap();
                child.parent = parent;
                child.genome = child.genome.with(Gene::Size, 40.0); // half grown
            }
        };
        let alone = best_prey(careful(50.0), 0.3, |w: &mut World| {
            small(w, 1060.0, None);
        })
        .unwrap();
        let covered = best_prey(careful(50.0), 0.3, with_parent(50.0)).unwrap();
        let forgotten = best_prey(careful(50.0), 0.3, with_parent(10.0)).unwrap();
        assert!(covered.score < alone.score, "a parent in sight did not count");
        assert_eq!(forgotten.score, alone.score, "a parent that forgot its child still covers it");
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
            near.set(small(w, 1030.0, None));
        })
        .unwrap();
        assert_eq!(best.id, near.get(), "the nearest prey was never looked at");
    }

    /// Whom a stranger fears: anyone whose program hunts bodies of its size (the threat), and of
    /// them the ones hunting somebody now (the hunter). A herbivore eats no one, and a carnivore
    /// whose program has no hunt block threatens no one either.
    #[test]
    fn a_threat_is_whoever_could_hunt_it_a_hunter_whoever_does() {
        use crate::creature::{Action, Block, Program};
        use crate::genome::creature::Gene;
        for (diet, hunts, hunting, threat, hunter) in [
            (SCAVENGER, true, false, true, false),
            (CARNIVORE, true, true, true, true),
            (CARNIVORE, false, true, false, false),
            (0.0, true, true, false, false),
        ] {
            let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
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
            let (any, hunts_now) =
                nearest_threats(&herd, v.kinship(), v.flock, v.x, v.y, v.pheno.size, 100.0);
            let case = format!("diet {diet}, hunt block {hunts}, hunting {hunting}");
            assert_eq!((any.is_some(), hunts_now.is_some()), (threat, hunter), "{case}");
            let (none, _) = nearest_threats(&herd, v.kinship(), v.flock, v.x, v.y, v.pheno.size, 50.0);
            assert!(none.is_none(), "{case}: farther than asked");
        }
    }
}
