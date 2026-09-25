//! Социальная память и локальные решения. Подготовка читает один снимок до движения.
use crate::{
    creature::{Creature, Intent, Me, Mind},
    genome::creature::Gene,
    grid::Grid,
    kin_grace::Grace,
    rng::mix,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Activity {
    Feeding,
    Resting,
    #[default]
    Travelling,
    Gathering,
    Alarm,
}
impl Activity {
    pub const ALL: [Self; 5] = [Self::Feeding, Self::Resting, Self::Travelling, Self::Gathering, Self::Alarm];
    pub fn label(self) -> &'static str {
        match self {
            Self::Feeding => "кормятся",
            Self::Resting => "отдыхают",
            Self::Travelling => "переходят",
            Self::Gathering => "собираются",
            Self::Alarm => "тревога",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Context {
    pub center: Option<(f64, f64)>,
    pub separation: (f64, f64),
    pub contact_separation: (f64, f64),
    pub food: Option<Food>,
    pub alarm: Option<Alarm>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Alarm {
    pub enemy: u64,
    pub x: f64,
    pub y: f64,
    pub tick: u64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aid {
    pub victim: u64,
    pub enemy: u64,
    pub started: u64,
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub alarms: u64,
    pub alarm_ends: u64,
    pub interventions: u64,
    pub splits: u64,
    pub departures: u64,
    /// Members that stayed outside their circle too long and left.
    pub strays: u64,
    /// Flocks that started a move to a new place: out of food, or no room.
    pub relocations: u64,
    /// Battles of flocks for room that started.
    pub battles: u64,
    /// Flocks that lost half of their fighters in a battle and moved away.
    pub battle_retreats: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Food {
    pub x: f64,
    pub y: f64,
    pub tick: u64,
    pub observer: u64,
}
impl Food {
    pub fn fresh(self, tick: u64) -> bool {
        tick.saturating_sub(self.tick) < 180
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Memory {
    pub last_shot: u64,
    pub territory_avoid: Option<crate::territory::Area>,
    pub territory_guard: Option<crate::territory::Guard>,
    /// Выбранная сторона обхода сохраняется, пока видна та же граница.
    pub territory_side: Option<(u64, i8)>,
    /// Курс выхода из пересекающихся областей держится до выхода из всех них.
    pub territory_escape: Option<(f64, f64)>,
    pub shared_flee: bool,
    pub rest_ready: u64,
    pub heading: Option<(f64, f64)>,
    pub personal_food: Option<(f64, f64)>,
    pub rejected_food: Option<Food>,
    pub observed_alarm: Option<Alarm>,
    pub alarm: Option<Alarm>,
    pub hit: Option<Alarm>,
    pub aid: Option<Aid>,
    pub aid_cooldown: u64,
    pub gathering: bool,
    /// Только личное наблюдение попадает в исходящее сообщение.
    pub observed_food: Option<Food>,
    pub food: Option<Food>,
    pub tick: u64,
    pub activity: Activity,
    pub context: Context,
    /// Since when the creature is outside its flock's circle (None: inside, or no circle).
    pub outside_since: Option<u64>,
    /// A hungry flock member forages outside its circle until it is fed again.
    pub foraging: bool,
    pub rest_until: u64,
    pub rest_count: u64,
    pub course: Option<(f64, f64)>,
    pub course_target: Option<(f64, f64)>,
    pub course_until: u64,
}

/// Ближайшие восемь своих, в стабильном порядке расстояние/ID.
pub fn neighbors(i: usize, creatures: &[Creature], grid: &Grid) -> Vec<usize> {
    let me = &creatures[i];
    let mut best: Vec<(f64, u64, usize)> = Vec::with_capacity(9);
    grid.for_each_near(me.x, me.y, me.pheno.vision, |j, x, y| {
        let v = &creatures[j];
        let d2 = (x - me.x).powi(2) + (y - me.y).powi(2);
        if i == j || !v.alive || v.flock != me.flock || d2 > me.pheno.vision2 {
            return;
        }
        let at = best.partition_point(|&(d, id, _)| (d, id) < (d2, v.id));
        if at < 8 {
            best.insert(at, (d2, v.id, j));
            best.truncate(8);
        }
    });
    best.into_iter().map(|(_, _, i)| i).collect()
}

pub fn prepare(creatures: &mut [Creature], grid: &Grid, tick: u64) {
    let contexts: Vec<_> = (0..creatures.len())
        .map(|i| {
            let me = &creatures[i];
            let near = neighbors(i, creatures, grid);
            let mut c = Context::default();
            let (mut x, mut y) = (0.0, 0.0);
            for &j in &near {
                let v = &creatures[j];
                if let Some(alarm) = v.mind.social.observed_alarm.filter(|a| tick.saturating_sub(a.tick) < 60)
                    && c.alarm.is_none_or(|old| {
                        (alarm.tick, std::cmp::Reverse(alarm.enemy))
                            > (old.tick, std::cmp::Reverse(old.enemy))
                    })
                {
                    c.alarm = Some(alarm);
                }
                if let Some(food) = v.mind.social.observed_food.filter(|f| f.fresh(tick))
                    && c.food.is_none_or(|old| {
                        (food.tick, std::cmp::Reverse(food.observer))
                            > (old.tick, std::cmp::Reverse(old.observer))
                    })
                {
                    c.food = Some(food);
                }
                x += v.x;
                y += v.y;
                let (dx, dy) = (me.x - v.x, me.y - v.y);
                let d = dx.hypot(dy);
                let wanted = me.pheno.size + v.pheno.half;
                if d < wanted {
                    let (ux, uy) = if d > 0.0 {
                        (dx / d, dy / d)
                    } else {
                        let angle = (mix(me.id.min(v.id)) % 360) as f64 * std::f64::consts::PI / 180.0;
                        let sign = if me.id < v.id { 1.0 } else { -1.0 };
                        (angle.cos() * sign, angle.sin() * sign)
                    };
                    c.separation.0 += ux * (1.0 - d / wanted.max(0.001));
                    c.separation.1 += uy * (1.0 - d / wanted.max(0.001));
                    let contact = me.pheno.half + v.pheno.half;
                    if d < contact {
                        c.contact_separation.0 += ux * (1.0 - d / contact.max(0.001));
                        c.contact_separation.1 += uy * (1.0 - d / contact.max(0.001));
                    }
                }
            }
            if !near.is_empty() {
                c.center = Some((x / near.len() as f64, y / near.len() as f64));
            }
            c
        })
        .collect();
    for (v, c) in creatures.iter_mut().zip(contexts) {
        v.mind.social.tick = tick;
        v.mind.social.context = c;
        v.mind.social.food = c
            .food
            .or(v.mind.social.food)
            .filter(|f| f.fresh(tick) && Some(*f) != v.mind.social.rejected_food);
        v.mind.social.alarm = c.alarm.or(v.mind.social.alarm).filter(|a| tick.saturating_sub(a.tick) < 60);
    }
}

fn unit(x: f64, y: f64) -> (f64, f64) {
    let d = x.hypot(y);
    if d > 0.0 { (x / d, y / d) } else { (0.0, 0.0) }
}

/// Готовность следовать чужой находке пропорциональна общительности.
/// Решение устойчиво 180 тиков, а фазы участников независимы.
pub fn follows_reports(me: &Me, mind: &Mind) -> bool {
    ((mix(me.kinship.id ^ (mind.social.tick / 180)) % 10000) as f64) < me.pheno.sociability * 10000.0
}

/// Social corrections of the chosen intent: flight together, aid, rest, gathering, separation
/// and a smooth turn. `returning`: the creature walks back into its flock's circle.
pub fn adjust(
    me: &Me,
    mind: &mut Mind,
    mut intent: Intent,
    feeding: bool,
    fleeing: bool,
    returning: bool,
) -> Intent {
    let m = &mut mind.social;
    m.shared_flee = false;
    let was_alarm = m.activity == Activity::Alarm;
    if fleeing {
        m.gathering = true;
        m.activity = if fleeing { Activity::Alarm } else { Activity::Feeding };
        m.rest_until = 0;
        m.course = None;
        return intent;
    }
    if let Some(aid) = m.aid {
        m.activity = Activity::Alarm;
        m.rest_until = 0;
        m.course = None;
        return Intent { tx: aid.x, ty: aid.y, slow: false, attack: Some(aid.enemy) };
    }
    let full = me.energy / me.pheno.max_energy;
    if me.pheno.sociability >= 0.25
        && !(full < 0.25 && feeding)
        && let Some(a) = m.alarm.filter(|a| m.tick.saturating_sub(a.tick) < 60)
    {
        let mut away = unit(me.x - a.x, me.y - a.y);
        if away == (0.0, 0.0) {
            let angle = (mix(me.kinship.id ^ a.enemy) % 360) as f64 * std::f64::consts::PI / 180.0;
            away = (angle.cos(), angle.sin());
        }
        if let Some((x, y)) = m.context.center {
            let toward = unit(x - me.x, y - me.y);
            if toward.0 * away.0 + toward.1 * away.1 >= 0.0 {
                away = unit(away.0 + 0.25 * toward.0, away.1 + 0.25 * toward.1);
            }
        }
        m.activity = Activity::Alarm;
        m.gathering = true;
        m.shared_flee = true;
        m.rest_until = 0;
        m.course = None;
        return Intent {
            tx: me.x + away.0 * me.pheno.speed,
            ty: me.y + away.1 * me.pheno.speed,
            slow: false,
            attack: None,
        };
    }
    if intent.attack.is_some() {
        m.activity = Activity::Feeding;
        m.rest_until = 0;
        m.course = None;
        return intent;
    }
    if was_alarm {
        m.heading = None;
    }
    let in_layer = me.y >= me.pheno.body_lo && me.y <= me.pheno.body_hi;
    // Home is the flock's circle for a member, the layer for anyone else.
    let at_home = me.circle.map_or(in_layer, |c| c.holds(me.x, me.y, 0.0));
    if full < 0.85 || !at_home {
        m.rest_until = 0;
    }
    if m.rest_until <= m.tick && m.tick >= m.rest_ready && full > 0.95 && at_home {
        m.rest_count += 1;
        m.rest_until = m.tick + 60 + mix(me.kinship.id ^ mix(m.rest_count)) % 61;
        m.rest_ready = m.rest_until + 180;
    }
    if m.rest_until > m.tick {
        m.activity = Activity::Resting;
        m.course = None;
        return Intent { tx: me.x, ty: me.y, slow: true, attack: None };
    }
    m.activity = if feeding { Activity::Feeding } else { Activity::Travelling };
    if !feeding && m.gathering {
        m.activity = Activity::Gathering;
        if m.context.center.is_some() {
            m.gathering = false;
        }
    }
    if returning {
        m.activity = Activity::Gathering;
    }
    // The circle keeps a flock together; there is no extra pull to the neighbours' centre.
    let mut dir = unit(intent.tx - me.x, intent.ty - me.y);
    if !feeding {
        if m.course_until > m.tick
            && m.course_target == Some((intent.tx, intent.ty))
            && let Some(course) = m.course
            && (intent.tx - me.x).hypot(intent.ty - me.y) > me.pheno.speed
        {
            dir = course;
        } else {
            m.course = Some(unit(dir.0, dir.1));
            m.course_target = Some((intent.tx, intent.ty));
            m.course_until = m.tick + 30;
        }
    } else {
        m.course = None;
    }
    let sep = if feeding { m.context.contact_separation } else { m.context.separation };
    if sep != (0.0, 0.0) || !feeding {
        let dir = unit(dir.0 + sep.0, dir.1 + sep.1);
        let distance = (intent.tx - me.x).hypot(intent.ty - me.y).min(me.pheno.speed);
        intent.tx = (me.x + dir.0 * distance).clamp(me.pheno.x_lo, me.pheno.x_hi);
        intent.ty = (me.y + dir.1 * distance).clamp(me.pheno.y_lo, me.pheno.y_hi);
    }
    // Плавный поворот спокойного хода; угроза и бой возвращаются выше.
    let (dx, dy) = (intent.tx - me.x, intent.ty - me.y);
    let distance = dx.hypot(dy);
    if let Some((hx, hy)) = m.heading
        && distance > 0.0
        && !feeding
        && (me.circle.is_some() || in_layer)
    {
        let old = hy.atan2(hx);
        let delta = (dy.atan2(dx) - old + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
            - std::f64::consts::PI;
        let angle = old + delta.clamp(-1.2, 1.2);
        intent.tx = me.x + angle.cos() * distance.min(me.pheno.speed);
        intent.ty = me.y + angle.sin() * distance.min(me.pheno.speed);
        if me.circle.is_none() {
            intent.ty = intent.ty.clamp(me.pheno.body_lo, me.pheno.body_hi);
        }
    }
    intent
}

/// Подтверждённые удары прошлого тика; не больше двух видящих помощников на жертву.
pub fn prepare_aid(creatures: &mut [Creature], grid: &Grid, tick: u64) -> u64 {
    prepare_aid_with_grace(creatures, grid, tick, &Grace::default())
}

/// Общая помощь после подтверждённого удара и забота родителя при лично
/// замеченной ребёнком угрозе. Оба вида помощи занимают те же два места.
pub fn prepare_aid_with_grace(creatures: &mut [Creature], grid: &Grid, tick: u64, grace: &Grace) -> u64 {
    let ids: std::collections::BTreeMap<_, _> =
        creatures.iter().enumerate().map(|(i, v)| (v.id, i)).collect();
    let mut assigned = vec![None; creatures.len()];
    let radius = creatures.iter().fold(0.0_f64, |r, v| r.max(v.pheno.vision));
    for victim in creatures.iter() {
        let direct_hit = victim.mind.social.hit.filter(|h| tick.saturating_sub(h.tick) < 30);
        let alarm = if victim.adult() {
            None
        } else {
            victim.mind.social.observed_alarm.filter(|a| tick.saturating_sub(a.tick) <= 1)
        };
        let Some(hit) = direct_hit.filter(|h| h.tick == tick).or(alarm).or(direct_hit) else { continue };
        let Some(&enemy_i) = ids.get(&hit.enemy) else { continue };
        let enemy = &creatures[enemy_i];
        if !enemy.alive || grace.contains(victim.flock, enemy.flock, tick) {
            continue;
        }
        let mut candidates = Vec::new();
        // Радиус жертвы не ограничивает зрение помощника: перебираем её локальную
        // окрестность по максимальному зрению, вычисленному один раз ниже вызывающим.
        grid.for_each_near(victim.x, victim.y, radius, |j, _, _| {
            let v = &creatures[j];
            let old = v.mind.social.aid;
            let continuing = old.is_some_and(|a| a.victim == victim.id && a.enemy == enemy.id);
            let care = (v.genome[Gene::Care] / 100.0).clamp(0.0, 1.0);
            let parent = !victim.adult() && victim.parent == v.id && care > 0.0;
            let flockmate = direct_hit.is_some_and(|h| h.enemy == hit.enemy)
                && v.flock == victim.flock
                && v.pheno.sociability >= 0.5;
            if assigned[j].is_some()
                || v.id == victim.id
                || !v.alive
                || !v.adult()
                || (!parent && !flockmate)
                || v.energy <= v.pheno.max_energy * 0.5
                || v.health / v.max_health() <= v.pheno.retreat + 0.1
                || v.kinship().kin(enemy.kinship())
                || v.flock == enemy.flock
                || grace.contains(v.flock, enemy.flock, tick)
                || tick < v.mind.social.aid_cooldown
                || (continuing && tick.saturating_sub(old.unwrap().started) >= 90)
                || (!continuing && tick != hit.tick)
            {
                return;
            }
            let d = (v.x - victim.x).hypot(v.y - victim.y);
            let range = if parent { v.pheno.vision * care } else { v.pheno.vision };
            if d <= range && (v.x - enemy.x).hypot(v.y - enemy.y) <= v.pheno.vision {
                candidates.push((d, v.id, j));
            }
        });
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (_, _, j) in candidates.into_iter().take(2) {
            let old = creatures[j].mind.social.aid.filter(|a| a.victim == victim.id && a.enemy == enemy.id);
            assigned[j] = Some(Aid {
                victim: victim.id,
                enemy: enemy.id,
                started: old.map_or(tick, |a| a.started),
                x: enemy.x,
                y: enemy.y,
            });
        }
    }
    let mut count = 0;
    for (v, aid) in creatures.iter_mut().zip(assigned) {
        let old = v.mind.social.aid;
        if old.is_some() && aid.is_none() {
            v.mind.attack = None;
            v.mind.social.aid_cooldown = tick + 60;
        }
        if aid.is_some() && old.is_none() {
            count += 1;
        }
        v.mind.social.aid = aid;
    }
    count
}

#[cfg(test)]
mod care_tests {
    use super::*;
    use crate::{CreatureGenome, World, WorldConfig};

    fn scenario(care: f64) -> (World, Grid, u64) {
        let mut world = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
        let parent = world.spawn(CreatureGenome::BASE.with(Gene::Care, care), 1000.0, 1000.0, Some(100.0));
        world.spawn(CreatureGenome::BASE, 1030.0, 1000.0, Some(20.0));
        world.creatures[1].parent = parent;
        world.creatures[1].pheno =
            crate::creature::Phenotype::at_size(&world.creatures[1].genome, &world.rules, &world.space, 20.0);
        world.creatures[1].birth_size = 20.0;
        let enemy = world.spawn(CreatureGenome::BASE.with(Gene::Size, 80.0), 1080.0, 1000.0, Some(100.0));
        world.creatures[1].mind.social.observed_alarm = Some(Alarm { enemy, x: 1080.0, y: 1000.0, tick: 1 });
        let mut grid = Grid::new(100.0);
        grid.rebuild(&world.space, world.creatures.iter().map(|v| (v.x, v.y)));
        (world, grid, enemy)
    }

    #[test]
    fn родитель_защищает_ребёнка_по_его_личной_тревоге_после_разделения_стай() {
        let (mut world, grid, enemy) = scenario(100.0);
        assert_ne!(world.creatures[0].flock, world.creatures[1].flock);
        assert_eq!(prepare_aid(&mut world.creatures, &grid, 1), 1);
        assert_eq!(world.creatures[0].mind.social.aid.unwrap().enemy, enemy);
        world.creatures[1].mind.social.observed_alarm = None;
        assert_eq!(prepare_aid(&mut world.creatures, &grid, 2), 0);
        assert!(world.creatures[0].mind.social.aid.is_none());
    }

    #[test]
    fn забота_здоровье_энергия_и_мир_разделившихся_ограничивают_защиту() {
        for reason in 0..4 {
            let (mut world, grid, _) = scenario(if reason == 0 { 0.0 } else { 100.0 });
            let mut grace = Grace::default();
            match reason {
                1 => world.creatures[0].energy = world.creatures[0].pheno.max_energy * 0.5,
                2 => world.creatures[0].health = 1.0,
                3 => grace.register(world.creatures[0].flock, world.creatures[2].flock, 0),
                _ => {}
            }
            assert_eq!(prepare_aid_with_grace(&mut world.creatures, &grid, 1, &grace), 0);
            assert!(world.creatures[0].mind.social.aid.is_none());
        }
    }
}
