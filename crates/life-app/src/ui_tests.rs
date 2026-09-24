//! Тесты экранов без окна (egui_kittest) — преемник `TestLayout` Python-версии.
//!
//! Каждый экран на наименьшем окне 960×600 и на обычном 1600×900: кнопки и
//! ползунки целиком в окне и не налезают друг на друга. Масштаб интерфейса ×2
//! на окне вдвое большего размера даёт ту же раскладку в точках, поэтому
//! отдельно не проверяется.
//!
//! С переменной TINYLIFE_SHOTS=папка тесты ещё и сохраняют картинки экранов —
//! чтобы посмотреть на них глазами.

use eframe::egui::accesskit::Role;
use eframe::egui::{Pos2, Rect, Vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use life_core::flora::Profile;
use life_core::{Shape, WorldConfig};

use crate::app::{LifeApp, Screen, SideTab};
use crate::frame::Instance;
use crate::settings::{Key, Tab};
use crate::sim::Command;
use crate::stats::StatsTab;

/// Видеокарта одна: параллельные рендеры wgpu в одном процессе роняют
/// драйвер на Windows, поэтому тесты экранов идут по очереди.
static GPU: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn gpu() -> std::sync::MutexGuard<'static, ()> {
    GPU.lock().unwrap_or_else(|e| e.into_inner())
}

const SMALL: Vec2 = Vec2::new(960.0, 600.0);
const NORMAL: Vec2 = Vec2::new(1600.0, 900.0);

fn harness(size: Vec2) -> Harness<'static, LifeApp> {
    harness_with(size, WorldConfig { seed: 7, ..Default::default() })
}

fn harness_with(size: Vec2, cfg: WorldConfig) -> Harness<'static, LifeApp> {
    let mut h =
        Harness::builder().with_size(size).wgpu().build_eframe(|cc| LifeApp::new(cc, Some(cfg), None));
    h.state_mut().sim.send(Command::SetPaused(true));
    // первый кадр мира приходит из потока симуляции; ждём ограниченно
    for _ in 0..300 {
        h.step();
        if h.state().view.frame.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(h.state().view.frame.is_some(), "кадр мира пришёл");
    // историю и хронику — чтобы графикам было что рисовать
    for _ in 0..600 {
        h.state_mut().sim.send(Command::Step);
    }
    for _ in 0..200 {
        h.step();
        if h.state().view.frame.as_ref().is_some_and(|f| f.tick >= 600) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    h
}

/// Прогнать несколько кадров окна (не `run`: поток симуляции будит окно сам,
/// и «пока не успокоится» не наступает).
fn settle(h: &mut Harness<'static, LifeApp>) {
    for _ in 0..6 {
        h.step();
    }
}

const ROLES: [Role; 6] =
    [Role::Button, Role::CheckBox, Role::Slider, Role::ComboBox, Role::Tab, Role::SpinButton];

/// Все кнопки, флажки и ползунки — целиком в окне и не налезают друг на друга.
/// `scrolled` — прокручиваемая область: что ушло из неё прокруткой, не ошибка
/// раскладки, и такие элементы не проверяются.
fn check_layout(h: &Harness<'static, LifeApp>, size: Vec2, what: &str, scrolled: Option<Rect>) {
    let window = Rect::from_min_size(Pos2::ZERO, size).expand(0.5);
    let mut widgets: Vec<(String, Rect)> = Vec::new();
    for role in ROLES {
        for node in h.query_all_by_role(role) {
            let label = node.accesskit_node().label().unwrap_or_default();
            let r = node.rect();
            if scrolled.is_some_and(|s| r.left() >= s.left() && !s.contains_rect(r)) {
                continue;
            }
            widgets.push((format!("{role:?} «{label}»"), r));
        }
    }
    assert!(!widgets.is_empty(), "{what}: нет ни одного элемента");
    for (name, r) in &widgets {
        assert!(window.contains_rect(*r), "{what}: {name} выходит за окно {size:?}: {r:?}");
    }
    for (i, (a, ra)) in widgets.iter().enumerate() {
        for (b, rb) in &widgets[i + 1..] {
            let overlap = ra.intersect(*rb);
            assert!(
                !overlap.is_positive() || overlap.area() < 1.0,
                "{what}: {a} {ra:?} налезает на {b} {rb:?}"
            );
        }
    }
}

fn shot(h: &mut Harness<'static, LifeApp>, name: &str) {
    let Ok(dir) = std::env::var("TINYLIFE_SHOTS") else { return };
    let image = h.render().expect("картинка экрана");
    std::fs::create_dir_all(&dir).expect("папка для картинок");
    image.save(format!("{dir}/{name}.png")).expect("картинка сохранилась");
}

fn each_size(check: impl Fn(&mut Harness<'static, LifeApp>, Vec2, &str)) {
    for (size, tag) in [(SMALL, "960x600"), (NORMAL, "1600x900")] {
        let mut h = harness(size);
        check(&mut h, size, tag);
    }
}

#[test]
fn игра_помещается_в_окно() {
    let _gpu = gpu();
    each_size(|h, size, tag| {
        for tab in [SideTab::Charts, SideTab::Log, SideTab::Creature] {
            h.state_mut().side_tab = tab;
            if tab == SideTab::Creature {
                // самый крупный кружок в кадре — существо (растения мелкие)
                let f = h.state().view.frame.as_ref().expect("кадр");
                let (x, y) = {
                    let i =
                        h.state().view.instances().iter().max_by(|a, b| a.r.total_cmp(&b.r)).expect("кружки");
                    (f.origin.0 + i.x as f64, f.origin.1 + i.y as f64)
                };
                h.state_mut().sim.send(Command::Pick { x, y, radius: 1.0 });
                for _ in 0..100 {
                    h.step();
                    if h.state().view.frame.as_ref().is_some_and(|f| f.selected.is_some()) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                h.state().view.frame.as_ref().and_then(|f| f.selected).expect("существо выбрано");
            }
            settle(h);
            // боковая панель прокручивается: от её вкладок до нижней панели
            let top = h.get_by_label("Графики").rect();
            let bottom = h.get_by_label("Выбор").rect().top();
            let panel =
                Rect::from_min_max(Pos2::new(top.left() - 12.0, top.top()), Pos2::new(size.x, bottom));
            check_layout(h, size, &format!("игра, {tab:?}, {tag}"), Some(panel));
            shot(h, &format!("игра-{tab:?}-{tag}"));
        }
    });
}

/// Крупный план: вблизи у существ кайма, ядро сытости и глазок
/// (`creatures.wgsl`). Шейдер компилируется и рисует — остальное видно на
/// картинке с TINYLIFE_SHOTS.
#[test]
fn крупный_план_рисуется() {
    let _gpu = gpu();
    let mut h = harness(NORMAL);
    h.state_mut().side_open = false;
    // там, где существ гуще всего: существо с наибольшим числом соседей
    let (x, y) = {
        let f = h.state().view.frame.as_ref().expect("кадр");
        let all = h.state().view.instances();
        let animals: Vec<_> =
            all.iter().filter(|i| (i.meta >> 16) & 3 != crate::motion::KIND_PLANT).collect();
        let near = |a: &Instance| {
            animals.iter().filter(|b| (a.x - b.x).abs() < 500.0 && (a.y - b.y).abs() < 300.0).count()
        };
        let i = animals.iter().max_by_key(|a| near(a)).expect("кружки");
        (f.origin.0 + i.x as f64, f.origin.1 + i.y as f64)
    };
    settle(&mut h);
    let generation = h.state().view.frame.as_ref().map(|f| f.tick);
    {
        let cam = h.state_mut().view.camera.as_mut().expect("камера");
        cam.zoom = 1.5;
        cam.center_on(x, y);
    }
    // кадр для нового вида приходит из потока симуляции
    for _ in 0..100 {
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(h.state().view.frame.as_ref().map(|f| f.tick), generation, "на паузе мир стоит");
    assert!(!h.state().view.instances().is_empty(), "вблизи есть кого рисовать");
    shot(&mut h, "крупный-план-1600x900");
}

/// Квадратный мир с едой волнами по ширине: вблизи видна миникарта квадратом,
/// и она не заслоняет инструменты.
#[test]
fn квадратный_мир_помещается_в_окно() {
    let _gpu = gpu();
    let rules = life_core::Rules::default().with_text("plant_width_profile", "waves").expect("профиль");
    for (size, tag) in [(SMALL, "960x600"), (NORMAL, "1600x900")] {
        let cfg = WorldConfig {
            seed: 7,
            scale: 10.0,
            shape: Shape::Square,
            rules: rules.clone(),
            ..Default::default()
        };
        let mut h = harness_with(size, cfg);
        h.state_mut().side_open = false;
        settle(&mut h);
        shot(&mut h, &format!("квадрат-весь-{tag}"));
        {
            let cam = h.state_mut().view.camera.as_mut().expect("камера");
            cam.zoom = cam.min_zoom() * 3.0;
        }
        for _ in 0..60 {
            h.step();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        check_layout(&h, size, &format!("квадратный мир, {tag}"), None);
        shot(&mut h, &format!("квадрат-вблизи-{tag}"));
    }
}

#[test]
fn лаборатория_на_ходу_помещается_в_окно() {
    let _gpu = gpu();
    each_size(|h, size, tag| {
        h.state_mut().lab_open = true;
        h.state_mut().side_open = false;
        for tab in [Tab::Lab, Tab::Food] {
            h.state_mut().lab_tab = tab;
            if tab == Tab::Food {
                h.state_mut().lab.set(Key::PlantWidthProfile, Profile::Waves.index());
            }
            settle(h);
            let window = Rect::from_min_size(Pos2::ZERO, size).expand(0.5);
            let apply = h.get_by_label("Применить").rect();
            assert!(
                window.contains_rect(apply),
                "лаборатория {tab:?}, {tag}: «Применить» за окном: {apply:?}"
            );
            for role in [Role::Slider, Role::ComboBox] {
                for node in h.query_all_by_role(role) {
                    assert!(
                        window.contains_rect(node.rect()),
                        "лаборатория {tab:?}, {tag}: {role:?} за окном"
                    );
                }
            }
            shot(h, &format!("лаборатория-{tab:?}-{tag}"));
        }
    });
}

#[test]
fn меню_и_новый_мир_помещаются_в_окно() {
    let _gpu = gpu();
    each_size(|h, size, tag| {
        h.state_mut().screen = Screen::Menu;
        settle(h);
        check_layout(h, size, &format!("меню, {tag}"), None);
        shot(h, &format!("меню-{tag}"));
        for tab in [Tab::World, Tab::Food, Tab::Lab] {
            h.state_mut().screen = Screen::Setup;
            h.state_mut().setup_tab = tab;
            settle(h);
            check_layout(h, size, &format!("новый мир, {tab:?}, {tag}"), None);
            shot(h, &format!("новый-мир-{tab:?}-{tag}"));
        }
        // волны по обеим осям: у каждой оси видны два параметра — самая длинная вкладка
        h.state_mut().setup_tab = Tab::Food;
        let s = &mut h.state_mut().settings;
        s.set(Key::PlantDepthProfile, Profile::Waves.index());
        s.set(Key::PlantWidthProfile, Profile::Waves.index());
        s.shape = Shape::Square;
        settle(h);
        check_layout(h, size, &format!("новый мир, еда волнами, {tag}"), None);
        shot(h, &format!("новый-мир-еда-волны-{tag}"));
    });
}

#[test]
fn справка_и_настройки_помещаются_в_окно() {
    let _gpu = gpu();
    each_size(|h, size, tag| {
        h.state_mut().screen = Screen::Menu;
        h.state_mut().help_open = true;
        settle(h);
        let window = Rect::from_min_size(Pos2::ZERO, size).expand(0.5);
        for role in [Role::Button] {
            for node in h.query_all_by_role(role) {
                assert!(window.contains_rect(node.rect()), "справка, {tag}: кнопка за окном");
            }
        }
        shot(h, &format!("справка-{tag}"));
        h.state_mut().help_open = false;
        h.state_mut().prefs_open = true;
        settle(h);
        for node in h.query_all_by_role(Role::CheckBox) {
            assert!(window.contains_rect(node.rect()), "настройки, {tag}: флажок за окном");
        }
        shot(h, &format!("настройки-{tag}"));
    });
}

#[test]
fn новый_мир_из_экрана_настроек_запускает_партию() {
    let _gpu = gpu();
    let mut h = harness(NORMAL);
    h.state_mut().screen = Screen::Setup;
    h.state_mut().settings.random_seed = false;
    h.state_mut().settings.seed = 4242;
    h.state_mut().settings.scale = 10.0;
    settle(&mut h);
    h.get_by_label("Начать").click();
    for _ in 0..300 {
        h.step();
        if h.state().view.frame.as_ref().is_some_and(|f| f.seed == 4242) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let f = h.state().view.frame.as_ref().expect("кадр");
    assert_eq!((f.seed, f.scale), (4242, 10.0));
    assert_eq!(h.state().screen, Screen::Game);
}

/// Окно «Статистика» на всех вкладках, с заданной областью: элементы окна
/// в окне программы и не налезают друг на друга.
#[test]
fn статистика_помещается_в_окно() {
    let _gpu = gpu();
    each_size(|h, size, tag| {
        h.state_mut().side_open = false;
        let (w, hh) = {
            let f = h.state().view.frame.as_ref().expect("кадр");
            (f.world_w, f.world_h)
        };
        h.state_mut().set_region((0.0, 0.0, w / 2.0, hh / 3.0));
        for _ in 0..100 {
            h.step();
            if h.state().region.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(h.state().region.is_some(), "сводка по области пришла и на паузе");
        for tab in [StatsTab::Energy, StatsTab::Where, StatsTab::Region] {
            h.state_mut().stats_tab = tab;
            settle(h);
            assert_eq!(h.state().view.area, Some((0.0, 0.0, w / 2.0, hh / 3.0)));
            check_layout(h, size, &format!("статистика, {tab:?}, {tag}"), None);
            shot(h, &format!("статистика-{tab:?}-{tag}"));
        }
        h.state_mut().stats_open = false;
        h.state_mut().side_open = true;
        for tab in [SideTab::Charts, SideTab::Log, SideTab::Creature] {
            h.state_mut().side_tab = tab;
            settle(h);
            assert!(h.state().view.area.is_some(), "смена вкладки сохраняет область");
        }
        h.get_by_label("Убрать рамку").click();
        settle(h);
        assert!(h.state().region.is_none() && h.state().view.area.is_none());
    });
}

#[test]
fn раскраска_стай_и_спокойный_профиль_работают_на_паузе() {
    let _gpu = gpu();
    each_size(|h, size, tag| {
        h.state_mut().side_open = false;
        settle(h);
        let tick = h.state().view.frame.as_ref().unwrap().tick;
        h.get_by_label("Стаи").click();
        for _ in 0..100 {
            h.step();
            let colors: std::collections::BTreeSet<_> = h
                .state()
                .view
                .instances()
                .iter()
                .filter(|v| (v.meta >> 16) & 3 == crate::motion::KIND_CREATURE)
                .map(|v| v.color & 0xFFFFFF)
                .collect();
            if colors.len() > 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let colors: std::collections::BTreeSet<_> = h
            .state()
            .view
            .instances()
            .iter()
            .filter(|v| (v.meta >> 16) & 3 == crate::motion::KIND_CREATURE)
            .map(|v| v.color & 0xFFFFFF)
            .collect();
        assert!(colors.len() > 2, "стаи имеют разные цвета");
        let areas = &h.state().view.frame.as_ref().unwrap().flock_areas;
        assert!(!areas.is_empty(), "кнопка включает области стай");
        assert!(areas.iter().all(|a| a.members >= 2 && a.radius.is_finite() && a.radius > 0.0));
        assert_eq!(h.state().view.frame.as_ref().unwrap().tick, tick);
        // Пакет из 600 шагов выполнен без ожидания: дать закончиться анимации рождения.
        std::thread::sleep(std::time::Duration::from_millis(800));
        settle(h);
        check_layout(h, size, "вид стай", None);
        shot(h, &format!("стаи-{tag}"));
        h.get_by_label("Стаи").click();
        for _ in 0..100 {
            h.step();
            if h.state().view.frame.as_ref().unwrap().flock_areas.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(h.state().view.frame.as_ref().unwrap().flock_areas.is_empty());
        assert_eq!(h.state().view.frame.as_ref().unwrap().tick, tick);
        h.get_by_label("Спокойнее").click();
        for _ in 0..100 {
            h.step();
            if h.state().view.frame.as_ref().unwrap().rules.cost_scale == 3.0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let f = h.state().view.frame.as_ref().unwrap();
        assert_eq!(f.rules.cost_scale, 3.0);
        assert_eq!(crate::sim::SPEEDS[f.status.speed_index], Some(30.0));
        assert_eq!(f.tick, tick);
        assert!(h.state().game.as_ref().unwrap().rules_changed);
    });
}

#[test]
fn стайные_сценарии_и_карточка_без_окна() {
    use life_core::{CreatureGenome, Rules, World, genome::creature::Gene, plant::Plant, social::Activity};
    let _gpu = gpu();
    let fixture = || {
        let mut w = World::new(&WorldConfig {
            seed: 42,
            n_creatures: Some(0),
            rules: Rules::default().with("plant_rate", 0.0).unwrap().with("cannibalism", 1.0).unwrap(),
            ..Default::default()
        });
        for (x, y) in [(2800.0, 1900.0), (2900.0, 2000.0), (3000.0, 2100.0), (3100.0, 1900.0)] {
            w.spawn(CreatureGenome::BASE, x, y, Some(65.0));
        }
        for v in &mut w.creatures {
            v.flock = 1;
            v.reproduction_wait = 10000;
        }
        w
    };
    let mut w = fixture();
    for x in [2870.0, 2990.0, 3070.0] {
        w.plants.push(Plant::at(x, 2000.0));
    }
    w.step();
    let mut scenes = vec![("кормёжка", w.clone())];
    for _ in 0..240 {
        w.step();
    }
    assert!(w.plants.is_empty());
    scenes.push(("переход", w));
    let mut w = fixture();
    w.spawn(CreatureGenome::BASE.with(Gene::Size, 120.0), 2950.0, 2000.0, Some(180.0));
    w.creatures.last_mut().unwrap().age = life_core::config::LIFESPAN - 12.0;
    let (mut alarm, mut gathering) = (false, false);
    for _ in 0..200 {
        w.step();
        if !alarm && w.creatures.iter().any(|v| v.mind.social.activity == Activity::Alarm) {
            scenes.push(("тревога", w.clone()));
            alarm = true;
        }
        if alarm
            && !gathering
            && w.creatures.iter().all(|v| v.mind.social.activity != Activity::Alarm)
            && w.creatures.iter().any(|v| v.mind.social.activity == Activity::Gathering)
        {
            scenes.push(("сбор", w.clone()));
            gathering = true;
        }
    }
    assert!(alarm && gathering, "после ухода угрозы стая собирается");
    for (size, tag) in [(SMALL, "960x600"), (NORMAL, "1600x900")] {
        let mut h = harness(size);
        h.state_mut().side_open = false;
        h.state_mut().flock_colors = true;
        for (name, w) in &scenes {
            let generation = h.state().view.frame.as_ref().unwrap().world_gen;
            h.state_mut().sim.send(Command::TestWorld(Box::new(w.clone())));
            for _ in 0..200 {
                h.step();
                if h.state()
                    .view
                    .frame
                    .as_ref()
                    .is_some_and(|f| f.world_gen > generation && !f.flock_areas.is_empty())
                {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(h.state().view.frame.as_ref().unwrap().world_gen > generation);
            if let Some(cam) = &mut h.state_mut().view.camera {
                let (sx, sy) = cam.to_screen(3000.0, 2000.0);
                cam.zoom_at(sx, sy, 2.5);
                cam.center_on(3000.0, 2000.0);
            }
            std::thread::sleep(std::time::Duration::from_millis(750));
            settle(&mut h);
            shot(&mut h, &format!("поведение-{name}-{tag}"));
        }
        let area = h.state().view.frame.as_ref().unwrap().flock_areas[0].clone();
        // Точка внутри области, но вне тел: выбор именно стаи.
        let w = &scenes.last().unwrap().1;
        let point = (0..36)
            .map(|i| {
                let a = i as f64 * std::f64::consts::TAU / 36.0;
                (area.x + area.radius * 0.7 * a.cos(), area.y + area.radius * 0.7 * a.sin())
            })
            .find(|&(x, y)| w.pick(x, y, 0.0).is_none())
            .unwrap();
        h.state_mut().sim.send(Command::Pick { x: point.0, y: point.1, radius: 0.0 });
        h.state_mut().side_open = true;
        h.state_mut().side_tab = SideTab::Creature;
        for _ in 0..100 {
            h.step();
            if h.state().view.frame.as_ref().unwrap().selected_flock.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(h.state().view.frame.as_ref().unwrap().selected_flock.as_ref().unwrap().id, area.id);
        settle(&mut h);
        check_layout(&h, size, "карточка стаи", None);
        shot(&mut h, &format!("карточка-стаи-{tag}"));
        let v = &w.creatures[0];
        h.state_mut().sim.send(Command::Pick { x: v.x, y: v.y, radius: 0.0 });
        for _ in 0..100 {
            h.step();
            if h.state().view.frame.as_ref().unwrap().selected.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(h.state().view.frame.as_ref().unwrap().selected_flock.is_none());
        assert_eq!(h.state().view.frame.as_ref().unwrap().selected.unwrap().id, v.id);

        let territory = fixture();
        let generation = h.state().view.frame.as_ref().unwrap().world_gen;
        h.state_mut().sim.send(Command::TestWorld(Box::new(territory.clone())));
        for _ in 0..100 {
            h.step();
            if h.state()
                .view
                .frame
                .as_ref()
                .is_some_and(|f| f.world_gen > generation && !f.flock_areas.is_empty())
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let area = h.state().view.frame.as_ref().unwrap().flock_areas[0].clone();
        assert!(area.territory_radius > area.radius);
        let outside_group = (0..72)
            .map(|i| {
                let angle = i as f64 * std::f64::consts::TAU / 72.0;
                (
                    area.x + area.territory_radius * 0.9 * angle.cos(),
                    area.y + area.territory_radius * 0.9 * angle.sin(),
                )
            })
            .find(|&(x, y)| {
                (x - area.x).hypot(y - area.y) > area.radius && territory.pick(x, y, 0.0).is_none()
            })
            .expect("виден участок территории вне стаи");
        h.state_mut().sim.send(Command::Pick { x: outside_group.0, y: outside_group.1, radius: 0.0 });
        for _ in 0..100 {
            h.step();
            if h.state().view.frame.as_ref().unwrap().selected_flock.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(h.state().view.frame.as_ref().unwrap().selected_flock.as_ref().unwrap().id, area.id);
        settle(&mut h);
        check_layout(&h, size, "территория стаи", None);
        shot(&mut h, &format!("территория-стаи-{tag}"));
    }
}

#[test]
fn следы_залпа_и_труп_рисуются_без_окна() {
    use life_core::{CreatureGenome, Rules, Shot, World, corpse::Corpse, genome::creature::Gene};
    let _gpu = gpu();
    let mut scene = World::new(&WorldConfig {
        seed: 43,
        n_creatures: Some(0),
        rules: Rules::default().with("plant_rate", 0.0).unwrap().with("cannibalism", 1.0).unwrap(),
        ..Default::default()
    });
    for x in [1000.0, 1020.0, 1040.0] {
        scene.spawn(
            CreatureGenome::BASE.with(Gene::Size, 40.0).with(Gene::Shooter, 1.0),
            x,
            1000.0,
            Some(90.0),
        );
    }
    let tag = scene.creatures[0].flock;
    for v in &mut scene.creatures {
        v.flock = tag;
    }
    scene.spawn(CreatureGenome::BASE.with(Gene::Size, 80.0), 1140.0, 1000.0, Some(130.0));
    scene.spawn(CreatureGenome::BASE, 1090.0, 1050.0, Some(65.0));
    let dead = scene.creatures.pop().unwrap();
    scene.corpses.push(Corpse::from_creature(&dead, 0));
    for from in [(1000.0, 1000.0), (1020.0, 1000.0), (1040.0, 1000.0)] {
        scene.shots.push(Shot { from, to: (1140.0, 1000.0), tick: 0 });
    }
    life_core::flock::update(&mut scene.flocks, &mut scene.creatures, &scene.space, 43, false);
    for (size, tag) in [(SMALL, "960x600"), (NORMAL, "1600x900")] {
        let mut h = harness(size);
        h.state_mut().side_open = false;
        h.state_mut().flock_colors = true;
        let generation = h.state().view.frame.as_ref().unwrap().world_gen;
        h.state_mut().sim.send(Command::TestWorld(Box::new(scene.clone())));
        for _ in 0..100 {
            h.step();
            if h.state().view.frame.as_ref().is_some_and(|f| f.world_gen > generation && f.shots.len() == 3) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let frame = h.state().view.frame.as_ref().unwrap();
        assert_eq!(frame.shots.len(), 3);
        assert_eq!(frame.corpses.len(), 1);
        assert_eq!(frame.flock_areas.len(), 1);
        if let Some(cam) = &mut h.state_mut().view.camera {
            let (sx, sy) = cam.to_screen(1070.0, 1020.0);
            cam.zoom_at(sx, sy, 5.0);
            cam.center_on(1070.0, 1020.0);
        }
        h.step();
        shot(&mut h, &format!("залп-и-труп-{tag}"));
    }
}

#[test]
fn последовательность_обхода_предупреждения_залпа_и_кормёжки_без_окна() {
    use life_core::{CreatureGenome, Rules, World, flock, genome::creature::Gene};
    let _gpu = gpu();
    let mut world = World::new(&WorldConfig {
        seed: 43,
        n_creatures: Some(0),
        rules: Rules::default().with("plant_rate", 0.0).unwrap().with("cannibalism", 1.0).unwrap(),
        ..Default::default()
    });
    let shooter = CreatureGenome::BASE
        .with(Gene::Shooter, 1.0)
        .with(Gene::FirePreference, 100.0)
        .with(Gene::FireReserve, 0.0);
    for x in [1000.0, 1020.0, 1040.0] {
        world.spawn(shooter, x, 1000.0, Some(100.0));
    }
    let home = world.creatures[0].flock;
    for v in &mut world.creatures[..3] {
        v.flock = home;
    }
    let enemy = world.spawn(CreatureGenome::BASE.with(Gene::Size, 80.0), 1140.0, 1000.0, Some(120.0));
    world.creatures[3].health = 0.8;
    flock::update(&mut world.flocks, &mut world.creatures, &world.space, 43, false);
    let mut scenes = vec![("граница", world.clone())];
    let before = world.creatures[3].x;
    world.step();
    assert!(world.creatures[3].x > before, "чужак уходит из чужой области");
    scenes.push(("обход", world));

    let mut warning = scenes[0].1.clone();
    warning.tick = 30;
    warning.territory.encounters.insert((home, enemy), 0);
    let targets = warning.territory.prepare(&mut warning.flocks, &mut warning.creatures, &warning.space, 30);
    assert_eq!(warning.flocks[&home].warned, 1);
    assert_eq!(targets.iter().filter(|&&target| target == Some(enemy)).count(), 3);
    scenes.push(("предупреждение", warning.clone()));
    warning.step();
    assert_eq!(warning.counters.ranged_shots, 3);
    assert_eq!(warning.corpses.len(), 1);
    scenes.push(("залп-и-труп", warning.clone()));
    let corpse = &warning.corpses[0];
    warning.creatures[0].x = corpse.x;
    warning.creatures[0].y = corpse.y;
    warning.creatures[0].energy = 20.0;
    warning.step();
    assert!(warning.counters.meat_bites > 0);
    scenes.push(("кормёжка", warning));

    for (size, tag) in [(SMALL, "960x600"), (NORMAL, "1600x900")] {
        let mut h = harness(size);
        h.state_mut().side_open = false;
        h.state_mut().flock_colors = true;
        for (name, scene) in &scenes {
            let generation = h.state().view.frame.as_ref().unwrap().world_gen;
            h.state_mut().sim.send(Command::TestWorld(Box::new(scene.clone())));
            for _ in 0..100 {
                h.step();
                if h.state().view.frame.as_ref().is_some_and(|f| f.world_gen > generation) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let frame = h.state().view.frame.as_ref().unwrap();
            assert!(frame.world_gen > generation);
            assert_eq!(frame.tick, scene.tick);
            if *name == "залп-и-труп" {
                assert_eq!(frame.shots.len(), 3);
                assert_eq!(frame.corpses.len(), 1);
            }
            if let Some(cam) = &mut h.state_mut().view.camera {
                let (sx, sy) = cam.to_screen(1070.0, 1000.0);
                cam.zoom_at(sx, sy, 9.0);
                cam.center_on(1070.0, 1000.0);
            }
            settle(&mut h);
            shot(&mut h, &format!("территория-{name}-{tag}"));
        }
    }
}

/// Игра по умолчанию — с каннибализмом: галочка есть в лаборатории.
#[test]
fn каннибализм_в_лаборатории() {
    let _gpu = gpu();
    let settings = crate::settings::Settings::default();
    for (size, tag) in [(SMALL, "960x600"), (NORMAL, "1600x900")] {
        let cfg = settings.world_config(7);
        assert!(cfg.rules.cannibals());
        let mut h = harness_with(size, cfg);
        h.state_mut().lab_open = true;
        h.state_mut().lab_tab = Tab::Lab;
        settle(&mut h);
        assert!(h.query_by_label("Каннибализм").is_some(), "{tag}: галочка каннибализма в лаборатории");
        shot(&mut h, &format!("лаборатория-каннибализм-{tag}"));
    }
}
