//! Flock circles as territories: others walk around them, and with combat on a territorial
//! flock warns and strikes intruders. Kinship and the grace after a split forbid only strikes:
//! everyone walks around a hard circle, and around a moderate one while it sees a member of
//! that flock (a leaky border: a sparse or too wide circle is not respected everywhere).
//! Fighters of a battle (`battle.rs`) get their targets here too.
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    Space,
    creature::{Creature, Intent},
    flock::{Flock, Territoriality},
    grid::Grid,
    kin_grace::Grace,
    rng::mix,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub flock: u64,
    pub x: f64,
    pub y: f64,
    pub radius: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guard {
    pub enemy: u64,
    pub x: f64,
    pub y: f64,
    pub half: f64,
}

#[derive(Clone, Debug)]
pub struct State {
    pub encounters: BTreeMap<(u64, u64), u64>,
    pub attacks: BTreeSet<(u64, u64, u64)>,
    areas: Vec<Area>,
    /// Whether the area of the same index is hard: always walked around.
    hard: Vec<bool>,
    grid: Grid,
    /// The largest area radius: how far a query must look.
    max_radius: f64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            encounters: BTreeMap::new(),
            attacks: BTreeSet::new(),
            areas: Vec::new(),
            hard: Vec::new(),
            grid: Grid::new(256.0),
            max_radius: 0.0,
        }
    }
}

impl State {
    pub fn clear(&mut self) {
        self.encounters.clear();
        self.attacks.clear();
        self.areas.clear();
    }

    /// В перекрытии владеет точкой ближайший нормированный центр.
    pub fn owner(&self, x: f64, y: f64) -> Option<Area> {
        self.owner_where(x, y, |_| true)
    }

    fn owner_where(&self, x: f64, y: f64, allowed: impl Fn(Area) -> bool) -> Option<Area> {
        if self.areas.is_empty() {
            return None;
        }
        let mut best: Option<(f64, u64, Area)> = None;
        self.grid.for_each_near(x, y, self.max_radius, |i, _, _| {
            let area = self.areas[i];
            if !allowed(area) {
                return;
            }
            let score = ((x - area.x).powi(2) + (y - area.y).powi(2)) / area.radius.powi(2);
            if score <= 1.0
                && best.is_none_or(|(old, id, _)| score < old || (score == old && area.flock < id))
            {
                best = Some((score, area.flock, area));
            }
        });
        best.map(|(_, _, area)| area)
    }

    pub fn prepare(
        &mut self,
        flocks: &mut BTreeMap<u64, Flock>,
        creatures: &mut [Creature],
        space: &Space,
        tick: u64,
    ) -> Vec<Option<u64>> {
        self.prepare_with_grace(flocks, creatures, space, tick, &Grace::default())
    }

    pub fn prepare_with_grace(
        &mut self,
        flocks: &mut BTreeMap<u64, Flock>,
        creatures: &mut [Creature],
        space: &Space,
        tick: u64,
        grace: &Grace,
    ) -> Vec<Option<u64>> {
        let mut herd = Grid::new(crate::config::GRID_CELL);
        herd.rebuild(space, creatures.iter().map(|v| (v.x, v.y)));
        self.prepare_full(flocks, creatures, space, tick, grace, true, &herd)
    }

    /// Areas of this tick and every creature's reaction to them. Without `combat` creatures
    /// still walk around circles, but nobody is warned or struck. `herd` is a grid over the
    /// creatures, in their order.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_full(
        &mut self,
        flocks: &mut BTreeMap<u64, Flock>,
        creatures: &mut [Creature],
        space: &Space,
        tick: u64,
        grace: &Grace,
        combat: bool,
        herd: &Grid,
    ) -> Vec<Option<u64>> {
        self.areas.clear();
        self.hard.clear();
        for (&tag, f) in flocks.iter() {
            if let Some(c) = f.circle.filter(|_| f.members >= 2 && f.territoriality != Territoriality::None) {
                self.areas.push(Area { flock: tag, x: c.x, y: c.y, radius: c.radius });
                self.hard.push(f.territoriality == Territoriality::Hard);
            }
        }
        self.max_radius = self.areas.iter().fold(0.0, |m, a| m.max(a.radius));
        self.grid.rebuild(space, self.areas.iter().map(|a| (a.x, a.y)));
        let owners: Vec<_> = if combat {
            creatures
                .iter()
                .map(|v| self.owner_where(v.x, v.y, |a| !grace.contains(a.flock, v.flock, tick)))
                .collect()
        } else {
            vec![None; creatures.len()]
        };
        if !combat {
            self.encounters.clear();
            self.attacks.clear();
        }
        let by_id: BTreeMap<_, _> = creatures.iter().enumerate().map(|(i, v)| (v.id, i)).collect();
        let mut active = BTreeSet::new();
        let mut immediate = BTreeSet::new();
        self.attacks.retain(|&(_, _, when)| when >= tick);
        for &(tag, enemy, when) in &self.attacks {
            if when == tick {
                immediate.insert((tag, enemy));
            }
        }
        for victim in creatures.iter().filter(|_| combat) {
            if let Some(hit) = victim.mind.social.hit.filter(|h| h.tick == tick)
                && let Some(&j) = by_id.get(&hit.enemy)
                && !grace.contains(victim.flock, creatures[j].flock, tick)
            {
                immediate.insert((victim.flock, hit.enemy));
            }
        }
        for (v, owner) in creatures.iter().zip(&owners) {
            if let Some(area) = owner.filter(|a| a.flock != v.flock) {
                active.insert((area.flock, v.id));
                self.encounters.entry((area.flock, v.id)).or_insert(tick);
            }
        }
        self.encounters.retain(|key, _| active.contains(key));
        let mut warned = BTreeMap::<u64, Vec<usize>>::new();
        for (i, v) in creatures.iter().enumerate() {
            if let Some(area) = owners[i].filter(|a| a.flock != v.flock) {
                let key = (area.flock, v.id);
                let mode = flocks[&area.flock].territoriality;
                if mode == Territoriality::Hard
                    || tick.saturating_sub(self.encounters[&key]) >= 30
                    || immediate.contains(&key)
                {
                    warned.entry(area.flock).or_default().push(i);
                }
            }
        }
        for (tag, enemy) in immediate {
            let Some(flock) = flocks.get(&tag).filter(|f| f.members >= 2) else {
                continue;
            };
            if let Some(&i) = by_id.get(&enemy) {
                if grace.contains(tag, creatures[i].flock, tick)
                    || flock.territoriality == Territoriality::None
                    || (flock.territoriality == Territoriality::Hard
                        && owners[i].is_none_or(|area| area.flock != tag))
                {
                    continue;
                }
                let enemies = warned.entry(tag).or_default();
                if !enemies.contains(&i) {
                    enemies.push(i);
                }
            }
        }
        for (&tag, f) in flocks.iter_mut() {
            f.warned = warned.get(&tag).map_or(0, Vec::len);
        }
        let mut targets = vec![None; creatures.len()];
        let mut near = Vec::new();
        let mut seen = Vec::new();
        for i in 0..creatures.len() {
            let v = &creatures[i];
            let mut guard = None;
            if combat && let Some(enemy) = battle_enemy(i, creatures, flocks, herd, grace, tick) {
                let u = &creatures[enemy];
                targets[i] = Some(u.id);
                guard = Some(Guard { enemy: u.id, x: u.x, y: u.y, half: u.pheno.half });
            } else if v.adult()
                && v.energy > v.pheno.max_energy * 0.5
                && v.health / v.max_health() > v.pheno.retreat + 0.1
                && let Some(enemies) = warned.get(&v.flock)
            {
                let best = enemies
                    .iter()
                    .copied()
                    .filter(|&j| {
                        let enemy = &creatures[j];
                        !v.kinship().kin(enemy.kinship())
                            && (enemy.x - v.x).hypot(enemy.y - v.y) <= v.pheno.vision
                    })
                    .max_by(|&a, &b| {
                        creatures[a]
                            .pheno
                            .size
                            .total_cmp(&creatures[b].pheno.size)
                            .then_with(|| {
                                let da = (creatures[a].x - v.x).hypot(creatures[a].y - v.y);
                                let db = (creatures[b].x - v.x).hypot(creatures[b].y - v.y);
                                db.total_cmp(&da)
                            })
                            .then_with(|| creatures[b].id.cmp(&creatures[a].id))
                    });
                if let Some(j) = best {
                    targets[i] = Some(creatures[j].id);
                    guard = Some(Guard {
                        enemy: creatures[j].id,
                        x: creatures[j].x,
                        y: creatures[j].y,
                        half: creatures[j].pheno.half,
                    });
                }
            }
            // A border is noticed locally, without a map of the world: areas within sight.
            near.clear();
            self.grid.for_each_near(v.x, v.y, v.pheno.vision + self.max_radius, |j, _, _| {
                let area = self.areas[j];
                if (v.x - area.x).hypot(v.y - area.y) - area.radius <= v.pheno.vision {
                    near.push(j);
                }
            });
            // A moderate circle is respected only while a member of it is in sight.
            seen.clear();
            if near.iter().any(|&j| !self.hard[j] && self.areas[j].flock != v.flock) {
                seen_flocks(v, creatures, herd, &mut seen);
            }
            // The circle it is walking around stays the one to avoid while it is in sight:
            // switching to a nearer neighbour on the way sent it straight back at the first one.
            let routed = v.mind.social.territory_side.map(|(tag, _)| tag);
            // Once it avoids a moderate circle it finishes the way out or around, even when the
            // member it saw drops out of sight: otherwise it turned back in and out every tick.
            let previous = v.mind.social.territory_avoid.map(|a| a.flock);
            let respected = |j: usize| {
                let area = self.areas[j];
                self.hard[j]
                    || seen.contains(&area.flock)
                    || previous == Some(area.flock)
                        && (routed == Some(area.flock)
                            || (v.x - area.x).hypot(v.y - area.y) < area.radius + v.pheno.half + 4.0)
            };
            let mut kept = None;
            let mut nearest: Option<(f64, u64, Area)> = None;
            let mut place: Option<(f64, u64, Area)> = None;
            let mut intrusions = Vec::new();
            for &j in &near {
                let area = self.areas[j];
                let distance = (v.x - area.x).hypot(v.y - area.y);
                // In an overlap the point belongs to the nearest normalised centre; the own
                // circle always counts.
                let score = distance.powi(2) / area.radius.powi(2);
                if score <= 1.0
                    && (area.flock == v.flock || respected(j))
                    && place.is_none_or(|(old, tag, _)| score < old || (score == old && area.flock < tag))
                {
                    place = Some((score, area.flock, area));
                }
                if area.flock == v.flock || !respected(j) {
                    continue;
                }
                let gap = distance - area.radius;
                if distance < area.radius + v.pheno.half + 4.0 {
                    intrusions.push(area);
                }
                if routed == Some(area.flock) {
                    kept = Some(area);
                }
                if nearest.is_none_or(|(old, tag, _)| gap < old || (gap == old && area.flock < tag)) {
                    nearest = Some((gap, area.flock, area));
                }
            }
            // At home the neighbour still counts: a member going out of its circle walks around
            // it (`steer` lets one moving inside its own circle be).
            let avoid = match place {
                Some((_, _, a)) if a.flock != v.flock => Some(a),
                _ => kept.or(nearest.map(|(_, _, a)| a)),
            };
            let old_escape = v.mind.social.territory_escape;
            let escape = if avoid.is_some() && !intrusions.is_empty() {
                old_escape.or_else(|| {
                    intrusions.sort_by_key(|a| a.flock);
                    if intrusions.len() >= 2 {
                        let a = intrusions[0];
                        let b = intrusions[1];
                        let (lx, ly) = (b.x - a.x, b.y - a.y);
                        let len = lx.hypot(ly);
                        if len > 1e-9 {
                            let (ux, uy) = (-ly / len, lx / len);
                            let position = (v.x - (a.x + b.x) * 0.5) * ux + (v.y - (a.y + b.y) * 0.5) * uy;
                            let side = if position.abs() > 1e-9 {
                                position.signum()
                            } else if mix(v.id ^ a.flock ^ b.flock) & 1 == 0 {
                                1.0
                            } else {
                                -1.0
                            };
                            return Some((ux * side, uy * side));
                        }
                    }
                    let a = intrusions[0];
                    let (dx, dy) = (v.x - a.x, v.y - a.y);
                    let d = dx.hypot(dy);
                    if d > 1e-9 {
                        Some((dx / d, dy / d))
                    } else {
                        let angle = (mix(v.id ^ a.flock) % 360) as f64 * std::f64::consts::TAU / 360.0;
                        Some((angle.cos(), angle.sin()))
                    }
                })
            } else {
                None
            };
            let mind = &mut creatures[i].mind.social;
            if mind.territory_avoid.map(|a| a.flock) != avoid.map(|a| a.flock) {
                mind.territory_side = None;
            }
            mind.territory_avoid = avoid;
            mind.territory_guard = guard;
            mind.territory_escape = escape;
        }
        targets
    }
}

/// Below this share of its tank a creature ignores borders: hunger outweighs the risk.
pub(crate) const STARVING_SHARE: f64 = 0.25;

/// Flocks with a member within the creature's sight.
fn seen_flocks(v: &Creature, creatures: &[Creature], herd: &Grid, out: &mut Vec<u64>) {
    herd.for_each_near(v.x, v.y, v.pheno.vision, |j, x, y| {
        let u = &creatures[j];
        if u.alive
            && u.flock != v.flock
            && (x - v.x).powi(2) + (y - v.y).powi(2) <= v.pheno.vision2
            && !out.contains(&u.flock)
        {
            out.push(u.flock);
        }
    });
}

/// The nearest adult of another flock of the same battle that creature `i` sees, if `i` is a
/// fighter: an adult, fed enough and not wounded, of a territorial flock (a flock without
/// territoriality only strikes back).
fn battle_enemy(
    i: usize,
    creatures: &[Creature],
    flocks: &BTreeMap<u64, Flock>,
    herd: &Grid,
    grace: &Grace,
    tick: u64,
) -> Option<usize> {
    let v = &creatures[i];
    let battle = flocks.get(&v.flock).filter(|f| f.territoriality != Territoriality::None)?.battle?;
    if !v.adult()
        || v.energy <= v.pheno.max_energy * crate::battle::FIGHTER_FULLNESS
        || v.health / v.max_health() <= v.pheno.retreat + 0.1
    {
        return None;
    }
    let mut best: Option<(f64, u64, usize)> = None;
    herd.for_each_near(v.x, v.y, v.pheno.vision, |j, x, y| {
        let u = &creatures[j];
        if !u.alive
            || !u.adult()
            || u.flock == v.flock
            || flocks.get(&u.flock).and_then(|f| f.battle) != Some(battle)
            || v.kinship().kin(u.kinship())
            || grace.contains(v.flock, u.flock, tick)
        {
            return;
        }
        let d2 = (x - v.x).powi(2) + (y - v.y).powi(2);
        if d2 <= v.pheno.vision2 && best.is_none_or(|(old, id, _)| d2 < old || (d2 == old && u.id < id)) {
            best = Some((d2, u.id, j));
        }
    });
    best.map(|(_, _, j)| j)
}

/// Если радиальный выход зажат границей мира, держим одну достижимую точку
/// выхода на его краю. Иначе в углу каждый тик тянет обратно к тому же углу.
fn edge_exit(v: &Creature, area: Area, radius: f64) -> Option<(f64, f64)> {
    let outer = radius + v.pheno.speed.max(1.0) + 1.0;
    let mut choices = Vec::with_capacity(12);
    for x in [v.pheno.x_lo, v.pheno.x_hi] {
        let dx = x - area.x;
        if dx.abs() <= outer {
            let dy = (outer * outer - dx * dx).sqrt();
            for y in [area.y - dy, area.y + dy] {
                choices.push((x, y.clamp(v.pheno.y_lo, v.pheno.y_hi)));
            }
        }
    }
    for y in [v.pheno.y_lo, v.pheno.y_hi] {
        let dy = y - area.y;
        if dy.abs() <= outer {
            let dx = (outer * outer - dy * dy).sqrt();
            for x in [area.x - dx, area.x + dx] {
                choices.push((x.clamp(v.pheno.x_lo, v.pheno.x_hi), y));
            }
        }
    }
    for x in [v.pheno.x_lo, v.pheno.x_hi] {
        for y in [v.pheno.y_lo, v.pheno.y_hi] {
            choices.push((x, y));
        }
    }
    choices
        .into_iter()
        .enumerate()
        .filter(|(_, (x, y))| (x - area.x).hypot(y - area.y) > radius + 1e-9)
        .min_by(|(ia, a), (ib, b)| {
            let da = (a.0 - v.x).hypot(a.1 - v.y);
            let db = (b.0 - v.x).hypot(b.1 - v.y);
            let shift = (v.id % 12) as usize;
            da.total_cmp(&db).then_with(|| ((ia + shift) % 12).cmp(&((ib + shift) % 12)))
        })
        .map(|(_, p)| p)
}

/// Продолжаем путь вдоль края после смены владельца перекрытой области.
fn follow_exit(v: &mut Creature, intent: &mut Intent, (tx, ty): (f64, f64)) {
    let (dx, dy) = (tx - v.x, ty - v.y);
    let distance = dx.hypot(dy);
    if distance > 1e-9 {
        v.mind.social.territory_escape = Some((dx / distance, dy / distance));
    }
    intent.tx = tx;
    intent.ty = ty;
}

/// Коррекция уже выбранного направления; не затрагивает самооборону и бегство.
pub fn steer(v: &mut Creature, mut intent: Intent) -> Intent {
    if v.fleeing() || v.mind.social.aid.is_some() {
        v.mind.social.territory_side = None;
        return intent;
    }
    if intent.attack.is_some() && intent.tx == v.x && intent.ty == v.y {
        v.mind.social.territory_side = None;
        return intent;
    }
    // A starving creature risks crossing a border for food or prey.
    if v.energy < v.pheno.max_energy * STARVING_SHARE {
        v.mind.social.territory_side = None;
        return intent;
    }
    if let Some(guard) = v.mind.social.territory_guard {
        v.mind.social.territory_side = None;
        let d = (guard.x - v.x).hypot(guard.y - v.y);
        let range = v.pheno.vision.min(v.pheno.size * 4.0);
        let contact = v.pheno.half + guard.half;
        let shot_cost = v.pheno.size * v.pheno.shot_energy_share;
        let can_pay =
            v.energy > shot_cost && v.energy - shot_cost >= v.pheno.max_energy * v.pheno.fire_reserve;
        let ready = if v.pheno.shooter && can_pay {
            contact + (range - contact).max(0.0) * v.pheno.fire_preference
        } else {
            contact
        };
        intent.tx = if d <= ready { v.x } else { guard.x };
        intent.ty = if d <= ready { v.y } else { guard.y };
        intent.slow = false;
        intent.attack = Some(guard.enemy);
        return intent;
    }
    let Some(area) = v.mind.social.territory_avoid else {
        return intent;
    };
    // At home: inside its own circle and going somewhere inside it. A neighbour's circle that
    // still overlaps while the two part does not push a member out of its own.
    if v.circle.is_some_and(|c| c.holds(v.x, v.y, 0.0) && c.holds(intent.tx, intent.ty, 0.0)) {
        v.mind.social.territory_side = None;
        return intent;
    }
    let (dx, dy) = (v.x - area.x, v.y - area.y);
    let d = dx.hypot(dy);
    let margin = v.pheno.half + 4.0;
    let r = area.radius + margin;
    if d < r {
        if let Some((ux, uy)) = v.mind.social.territory_escape {
            intent.tx = (v.x + ux * v.pheno.speed).clamp(v.pheno.x_lo, v.pheno.x_hi);
            intent.ty = (v.y + uy * v.pheno.speed).clamp(v.pheno.y_lo, v.pheno.y_hi);
            // A wall of the world blocks the way out (a circle pressed against the edge leaves a
            // corridor narrower than the body): slide along the wall on the side that leaves the
            // border, and keep it as the course out of every area it is in (like `follow_exit`),
            // instead of stepping back and forth across the margin.
            if d > 1e-9 && (intent.tx - v.x).hypot(intent.ty - v.y) < v.pheno.speed * 0.5 {
                let slide = |side: f64| {
                    (
                        (v.x - dy / d * side * v.pheno.speed).clamp(v.pheno.x_lo, v.pheno.x_hi),
                        (v.y + dx / d * side * v.pheno.speed).clamp(v.pheno.y_lo, v.pheno.y_hi),
                    )
                };
                let away = |p: (f64, f64)| (p.0 - area.x).hypot(p.1 - area.y);
                let stored = match v.mind.social.territory_side {
                    Some((tag, side)) if tag == area.flock => side,
                    _ => 1,
                };
                let (kept, other) = (slide(f64::from(stored)), slide(-f64::from(stored)));
                let (side, target) =
                    if away(other) > away(kept) + 1e-9 { (-stored, other) } else { (stored, kept) };
                if away(target) > d + 1e-9 {
                    v.mind.social.territory_side = Some((area.flock, side));
                    follow_exit(v, &mut intent, target);
                }
            }
            if (intent.tx - v.x).hypot(intent.ty - v.y) < v.pheno.speed * 0.1
                && let Some((tx, ty)) = edge_exit(v, area, r)
            {
                follow_exit(v, &mut intent, (tx, ty));
            }
        } else {
            let (ux, uy) = if d > 1e-9 {
                (dx / d, dy / d)
            } else {
                let angle = (mix(v.id ^ area.flock) % 360) as f64 * std::f64::consts::TAU / 360.0;
                (angle.cos(), angle.sin())
            };
            intent.tx = (area.x + ux * (r + v.pheno.speed)).clamp(v.pheno.x_lo, v.pheno.x_hi);
            intent.ty = (area.y + uy * (r + v.pheno.speed)).clamp(v.pheno.y_lo, v.pheno.y_hi);
            if (intent.tx - area.x).hypot(intent.ty - area.y) <= r + 1e-9
                && let Some((tx, ty)) = edge_exit(v, area, r)
            {
                follow_exit(v, &mut intent, (tx, ty));
            }
        }
        intent.attack = None;
        intent.slow = false;
    } else {
        let (tx, ty) = (intent.tx - v.x, intent.ty - v.y);
        if (intent.tx - area.x).hypot(intent.ty - area.y) < r {
            // An unreachable target is walked around on a stable side with an outward bias. A
            // straight retreat made it dart between the moving border and the old target; the
            // bias keeps it from circling the border forever.
            let radial = (dx / d, dy / d);
            let mut side = match v.mind.social.territory_side {
                Some((tag, side)) if tag == area.flock => side,
                _ => {
                    if let Some((hx, hy)) = v.mind.social.heading {
                        if (-radial.1 * hx + radial.0 * hy) >= 0.0 { 1 } else { -1 }
                    } else if mix(v.id ^ area.flock) & 1 == 0 {
                        1
                    } else {
                        -1
                    }
                }
            };
            let heading = v.mind.social.heading;
            let route = |side: i8| {
                let side = f64::from(side);
                let tangent = (-radial.1 * side, radial.0 * side);
                // Coming in towards the circle, the outward bias grows only as far as the step
                // does not turn back against the last one: a full bias at once was a sharp turn.
                let bias = match heading {
                    Some((hx, hy)) if hx * radial.0 + hy * radial.1 < 0.0 => {
                        let along = (hx * tangent.0 + hy * tangent.1).max(0.0);
                        (along / -(hx * radial.0 + hy * radial.1)).min(1.0)
                    }
                    _ => 1.0,
                };
                let (ux, uy) = (tangent.0 + radial.0 * bias, tangent.1 + radial.1 * bias);
                let length = ux.hypot(uy);
                (
                    (v.x + ux / length * v.pheno.speed).clamp(v.pheno.x_lo, v.pheno.x_hi),
                    (v.y + uy / length * v.pheno.speed).clamp(v.pheno.y_lo, v.pheno.y_hi),
                )
            };
            let mut target = route(side);
            if (target.0 - v.x).hypot(target.1 - v.y) < v.pheno.speed * 0.1 {
                side = -side;
                target = route(side);
            }
            // A side that a wall squeezes into the border is closed: go around the other way.
            let closed = |p: (f64, f64)| (p.0 - area.x).hypot(p.1 - area.y) < r;
            if closed(target) {
                let other = route(-side);
                if !closed(other) {
                    side = -side;
                    target = other;
                }
            }
            v.mind.social.territory_side = Some((area.flock, side));
            intent.tx = target.0;
            intent.ty = target.1;
            intent.attack = None;
            intent.slow = false;
            v.mind.social.personal_food = None;
            v.mind.target = None;
            return intent;
        }
        let length = tx.hypot(ty);
        let step = length.min(v.pheno.speed);
        if step > 0.0 {
            let nx = v.x + tx / length * step;
            let ny = v.y + ty / length * step;
            let projection = ((area.x - v.x) * tx + (area.y - v.y) * ty) / (length * length);
            let projection = projection.clamp(0.0, 1.0);
            let closest = (v.x + projection * tx - area.x).hypot(v.y + projection * ty - area.y);
            let blocked = closest < r + 1.0;
            let near_edge = (nx - area.x).hypot(ny - area.y) < r;
            let continuing = v.mind.social.territory_side.is_some_and(|(tag, _)| tag == area.flock);
            if blocked && (near_edge || continuing) {
                let cross = dx * ty - dy * tx;
                let mut side = match v.mind.social.territory_side {
                    Some((tag, side)) if tag == area.flock => side,
                    _ => {
                        let side = if cross.abs() < 1e-9 {
                            if mix(v.id ^ area.flock) & 1 == 0 { 1 } else { -1 }
                        } else if cross > 0.0 {
                            1
                        } else {
                            -1
                        };
                        v.mind.social.territory_side = Some((area.flock, side));
                        side
                    }
                } as f64;
                let tangent = |side: f64| {
                    (
                        (v.x - dy / d * side * step).clamp(v.pheno.x_lo, v.pheno.x_hi),
                        (v.y + dx / d * side * step).clamp(v.pheno.y_lo, v.pheno.y_hi),
                    )
                };
                let mut target = tangent(side);
                let closed = |p: (f64, f64)| (p.0 - area.x).hypot(p.1 - area.y) < r;
                if (target.0 - v.x).hypot(target.1 - v.y) <= 1e-9
                    || (closed(target) && !closed(tangent(-side)))
                {
                    side = -side;
                    target = tangent(side);
                    v.mind.social.territory_side = Some((area.flock, side as i8));
                }
                intent.tx = target.0;
                intent.ty = target.1;
                if (intent.tx - v.x).hypot(intent.ty - v.y) <= 1e-9
                    && let Some(exit) = edge_exit(v, area, r)
                {
                    follow_exit(v, &mut intent, exit);
                }
                intent.attack = None;
                intent.slow = false;
            } else if !blocked {
                v.mind.social.territory_side = None;
            }
        }
    }
    intent
}
