//! Подвижные территории стай: предупреждение вторженца и локальный обход границы.
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    Space,
    creature::{Creature, Intent},
    flock::Flock,
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
    grid: Grid,
}

impl Default for State {
    fn default() -> Self {
        Self {
            encounters: BTreeMap::new(),
            attacks: BTreeSet::new(),
            areas: Vec::new(),
            grid: Grid::new(256.0),
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
        self.grid.for_each_near(x, y, 320.0, |i, _, _| {
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
        self.areas.clear();
        self.areas.extend(flocks.iter().filter_map(|(&tag, f)| {
            (f.members >= 2 && f.territory_radius > 0.0).then_some(Area {
                flock: tag,
                x: f.goal.x,
                y: f.goal.y,
                radius: f.territory_radius,
            })
        }));
        self.grid.rebuild(space, self.areas.iter().map(|a| (a.x, a.y)));
        let owners: Vec<_> = creatures
            .iter()
            .map(|v| self.owner_where(v.x, v.y, |a| !grace.contains(a.flock, v.flock, tick)))
            .collect();
        let by_id: BTreeMap<_, _> = creatures.iter().enumerate().map(|(i, v)| (v.id, i)).collect();
        let mut active = BTreeSet::new();
        let mut immediate = BTreeSet::new();
        self.attacks.retain(|&(_, _, when)| when >= tick);
        for &(tag, enemy, when) in &self.attacks {
            if when == tick {
                immediate.insert((tag, enemy));
            }
        }
        for victim in creatures.iter() {
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
                if mode == crate::flock::Territoriality::Hard
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
                    || flock.territoriality == crate::flock::Territoriality::None
                    || (flock.territoriality == crate::flock::Territoriality::Hard
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
        for i in 0..creatures.len() {
            let v = &creatures[i];
            let mut guard = None;
            if v.adult()
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
            // Чужую границу замечают локально, не зная всей карты мира.
            let mut nearest: Option<(f64, u64, Area)> = None;
            let mut intrusions = Vec::new();
            self.grid.for_each_near(v.x, v.y, v.pheno.vision + 320.0, |j, _, _| {
                let area = self.areas[j];
                if area.flock == v.flock || grace.contains(area.flock, v.flock, tick) {
                    return;
                }
                let distance = (v.x - area.x).hypot(v.y - area.y);
                let gap = distance - area.radius;
                if gap > v.pheno.vision {
                    return;
                }
                if distance < area.radius + v.pheno.half + 4.0 {
                    intrusions.push(area);
                }
                if nearest.is_none_or(|(old, tag, _)| gap < old || (gap == old && area.flock < tag)) {
                    nearest = Some((gap, area.flock, area));
                }
            });
            // Своя территория имеет приоритет в перекрытии.
            let avoid = match owners[i] {
                Some(a) if a.flock != v.flock => Some(a),
                Some(_) => None,
                None => nearest.map(|(_, _, a)| a),
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
    // Очень голодный охотник может рискнуть и нарушить границу ради добычи.
    if intent.attack.is_some() && v.energy < v.pheno.max_energy * 0.25 {
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
    let (dx, dy) = (v.x - area.x, v.y - area.y);
    let d = dx.hypot(dy);
    let margin = v.pheno.half + 4.0;
    let r = area.radius + margin;
    if d < r {
        if let Some((ux, uy)) = v.mind.social.territory_escape {
            intent.tx = (v.x + ux * v.pheno.speed).clamp(v.pheno.x_lo, v.pheno.x_hi);
            intent.ty = (v.y + uy * v.pheno.speed).clamp(v.pheno.y_lo, v.pheno.y_hi);
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
            // Недоступную цель обходим по устойчивой стороне с уклоном наружу.
            // Прямой отход заставлял существо метаться между движущейся границей
            // и прежней целью. Уклон наружу не даёт зациклиться на окружности.
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
            let route = |side: i8| {
                let side = f64::from(side);
                let (ux, uy) = (radial.0 - radial.1 * side, radial.1 + radial.0 * side);
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
                if (target.0 - v.x).hypot(target.1 - v.y) <= 1e-9 {
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
