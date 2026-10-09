//! Screen tests without a window (egui_kittest) — the successor of the Python version's `TestLayout`.
//!
//! Every screen on the smallest window 960×600 and on a usual one 1600×900: buttons and sliders
//! whole inside the window and not overlapping one another. An interface scale of ×2 on a window
//! twice the size gives the same layout in points, so it is not checked separately.
//!
//! With the variable LIFEGAME_SHOTS=folder the tests also save pictures of the screens — to look
//! at them with one's own eyes.

use eframe::egui::accesskit::Role;
use eframe::egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use life_core::flora::Profile;
use life_core::{Shape, WorldConfig};

use crate::app::{LifeApp, Screen, SideTab, Tool};
use crate::frame::Instance;
use crate::settings::{Key, Tab};
use crate::sim::Command;

/// Diet gene values, by the variants' order.
const CARNIVORE: f64 = life_core::creature::Diet::Carnivore as usize as f64;
const SCAVENGER: f64 = life_core::creature::Diet::Scavenger as usize as f64;
use crate::stats::StatsTab;

/// There is one graphics card: parallel wgpu renders in one process crash the driver on
/// Windows, so the screen tests go one after another.
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
    // the world's first frame comes from the simulation thread; we wait a limited time
    for _ in 0..300 {
        h.step();
        if h.state().view.frame.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(h.state().view.frame.is_some(), "кадр мира пришёл");
    // the history and the chronicle — so that the charts have something to draw
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

/// Run a few window frames (not `run`: the simulation thread wakes the window by itself, and
/// «until it settles» never comes).
fn settle(h: &mut Harness<'static, LifeApp>) {
    for _ in 0..6 {
        h.step();
    }
}

const ROLES: [Role; 6] =
    [Role::Button, Role::CheckBox, Role::Slider, Role::ComboBox, Role::Tab, Role::SpinButton];

/// All buttons, checkboxes and sliders are whole inside the window and do not overlap.
/// `scrolled` — a scrollable area: what left it by scrolling is not a layout error, and such
/// elements are not checked.
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
    let Ok(dir) = std::env::var("LIFEGAME_SHOTS") else { return };
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
                // the biggest creature in the frame (a big meat-eating founder's corpse is not one)
                let f = h.state().view.frame.as_ref().expect("кадр");
                let (world_gen, frame) = (f.world_gen, f.number);
                let (x, y) = {
                    let i = h
                        .state()
                        .view
                        .instances()
                        .iter()
                        .filter(|v| (v.meta >> 16) & 3 == crate::motion::KIND_CREATURE)
                        .max_by(|a, b| a.r.total_cmp(&b.r))
                        .expect("кружки");
                    (f.origin.0 + i.x as f64, f.origin.1 + i.y as f64)
                };
                // at the end of their way, where the instance's own position is
                h.state_mut().sim.send(Command::Pick { x, y, radius: 1.0, world_gen, frame, k: 1.0 });
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
            // the side panel scrolls: from its tabs to the bottom panel
            let top = h.get_by_label("Графики").rect();
            let bottom = h.get_by_label("Выбор").rect().top();
            let panel =
                Rect::from_min_max(Pos2::new(top.left() - 12.0, top.top()), Pos2::new(size.x, bottom));
            check_layout(h, size, &format!("игра, {tab:?}, {tag}"), Some(panel));
            shot(h, &format!("игра-{tab:?}-{tag}"));
            if tab == SideTab::Charts {
                // a click on a diet's row opens how it lives next to the whole world
                assert!(h.query_by_label("сытость").is_none());
                h.get_by_label("травоядные").click();
                settle(h);
                assert_eq!(h.state().diet_open, [true, false, false, false]);
                assert!(h.query_by_label("сытость").is_some(), "the herbivores' details are open");
                check_layout(h, size, &format!("игра, питание, {tag}"), Some(panel));
                shot(h, &format!("игра-питание-{tag}"));
                h.state_mut().diet_open = [false; 4];
                // highlighting a diet lights it up in the world and fades the rest
                h.get_by_label("● всеяд.").click();
                settle(h);
                assert_eq!(h.state().view.highlight, 0b10);
                shot(h, &format!("игра-подсветка-{tag}"));
                h.get_by_label("● всеяд.").click();
                settle(h);
                assert_eq!(h.state().view.highlight, 0);
            }
        }
        // the price of a tick and, below it, where its time goes
        h.state_mut().settings.show_fps = true;
        settle(h);
        let top = h.get_by_label("Графики").rect();
        let bottom = h.get_by_label("Выбор").rect().top();
        let panel = Rect::from_min_max(Pos2::new(top.left() - 12.0, top.top()), Pos2::new(size.x, bottom));
        check_layout(h, size, &format!("игра, цена тика, {tag}"), Some(panel));
        let phases = h.get_by_label_contains("фазы тика:").rect();
        let window = Rect::from_min_size(Pos2::ZERO, size).expand(0.5);
        assert!(window.contains_rect(phases), "фазы тика выходят за окно {size:?}: {phases:?}");
        shot(h, &format!("игра-цена-тика-{tag}"));
        h.state_mut().settings.show_fps = false;
    });
}

/// A close-up: near, creatures have a rim, a fullness core and an eye (`creatures.wgsl`). The
/// shader compiles and draws — the rest is visible in the picture with LIFEGAME_SHOTS.
#[test]
fn крупный_план_рисуется() {
    let _gpu = gpu();
    let mut h = harness(NORMAL);
    h.state_mut().side_open = false;
    // where the creatures are thickest: the creature with the most neighbours
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
    // the frame for the new view comes from the simulation thread
    for _ in 0..100 {
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(h.state().view.frame.as_ref().map(|f| f.tick), generation, "на паузе мир стоит");
    assert!(!h.state().view.instances().is_empty(), "вблизи есть кого рисовать");
    shot(&mut h, "крупный-план-1600x900");
}

/// A square world with food in waves across the width: near, the minimap is seen as a square,
/// and it does not cover the tools.
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
        for tab in Tab::RULES.map(|(tab, _)| tab) {
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
            for role in [Role::SpinButton, Role::ComboBox] {
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
        for tab in std::iter::once(Tab::World).chain(Tab::RULES.map(|(tab, _)| tab)) {
            h.state_mut().screen = Screen::Setup;
            h.state_mut().setup_tab = tab;
            settle(h);
            check_layout(h, size, &format!("новый мир, {tab:?}, {tag}"), None);
            shot(h, &format!("новый-мир-{tab:?}-{tag}"));
        }
        // waves along both axes: each axis shows two parameters — the longest tab
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
        let inside = |h: &Harness<'static, LifeApp>, what: &str| {
            for role in [Role::CheckBox, Role::RadioButton, Role::Slider, Role::Button, Role::ComboBox] {
                for node in h.query_all_by_role(role) {
                    let label = node.accesskit_node().label().unwrap_or_default();
                    assert!(window.contains_rect(node.rect()), "{what}, {tag}: {role:?} «{label}» за окном");
                }
            }
        };
        inside(h, "настройки");
        shot(h, &format!("настройки-{tag}"));
        // by hand: a slider within the processor's threads; back to the defaults with one button
        h.get_by_label("Вручную").click();
        settle(h);
        let (cpu, auto) = (crate::settings::cpu_threads(), crate::settings::auto_threads());
        assert_eq!(h.state().settings.threads, auto.clamp(2, cpu), "вручную начинается с авто");
        assert_eq!(h.query_all_by_role(Role::Slider).count(), 1, "вручную — ползунок");
        inside(h, "настройки вручную");
        shot(h, &format!("настройки-вручную-{tag}"));
        // every thread to the world: a warning, and the window still fits
        h.state_mut().settings.threads = cpu;
        settle(h);
        assert!(h.query_by_label_contains("Окну почти не остаётся").is_some(), "предупреждение");
        inside(h, "настройки, все потоки");
        shot(h, &format!("настройки-все-потоки-{tag}"));
        h.get_by_label("Как по умолчанию").click();
        settle(h);
        assert_eq!(h.state().settings.threads, 0, "снова авто");
        assert_eq!(h.query_all_by_role(Role::Slider).count(), 0, "в авто ползунка нет");
        // the simulation takes what the settings say
        h.get_by_label("Один поток: медленнее, зато процессор свободен").click();
        for _ in 0..200 {
            h.step();
            if h.state().view.frame.as_ref().is_some_and(|f| f.threads == 1) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(h.state().view.frame.as_ref().map(|f| f.threads), Some(1), "мир считается одним потоком");
        h.state_mut().prefs_open = false;
    });
}

#[test]
fn лаборатория_сбрасывает_отмеченные_цены_и_показывается_без_окна() {
    let _gpu = gpu();
    each_size(|h, size, tag| {
        h.state_mut().lab_open = true;
        h.state_mut().lab_tab = Tab::Combat;
        h.state_mut().lab.set(Key::ShotDamage, 0.05);
        h.state_mut().lab.set(Key::ShotCost, 0.10);
        h.state_mut().lab_reset_selected.extend([Key::ShotDamage, Key::ShotCost]);
        settle(h);
        let screen = Rect::from_min_size(Pos2::ZERO, size).expand(0.5);
        for label in ["Применить", "Отменить", "Сбросить отмеченные"] {
            let node = h.get_by_label(label);
            assert!(
                screen.contains_rect(node.rect()),
                "лаборатория, {tag}: {label} за окном: {:?}",
                node.rect()
            );
        }
        shot(h, &format!("лаборатория-{tag}"));
        h.get_by_label("Сбросить отмеченные").click();
        settle(h);
        assert_eq!(
            h.state().lab.get(Key::ShotDamage),
            crate::settings::Settings::default().get(Key::ShotDamage)
        );
        assert_eq!(h.state().lab.get(Key::ShotCost), crate::settings::Settings::default().get(Key::ShotCost));
        assert!(h.state().lab_reset_selected.is_empty());
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

/// The «Статистика» window on all tabs, with a region set: the window's elements are inside the
/// program's window and do not overlap.
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
            if tab != StatsTab::Region {
                assert!(h.query_by_label("Последние 10 000 тиков").is_some());
            }
            check_layout(h, size, &format!("статистика, {tab:?}, {tag}"), None);
            shot(h, &format!("статистика-{tab:?}-{tag}"));
        }
        // the census comes on pause, on request, and its tab fits too
        h.state_mut().stats_tab = StatsTab::Species;
        for _ in 0..200 {
            h.step();
            if h.state().census.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(h.state().census.is_some(), "перепись пришла на паузе");
        settle(h);
        assert!(h.query_by_label("Признаки").is_some());
        // the tab is longer than the window: what is scrolled away below the tabs is not checked
        let tabs = h.get_by_label("Внутри видов").rect();
        let scroll = Rect::from_min_max(Pos2::new(0.0, tabs.bottom()), size.to_pos2());
        check_layout(h, size, &format!("статистика, внутри видов, {tag}"), Some(scroll));
        shot(h, &format!("статистика-Species-{tag}"));
        h.state_mut().stats_open = false;
        h.state_mut().side_open = true;
        for tab in [SideTab::Charts, SideTab::Log, SideTab::Creature] {
            h.state_mut().side_tab = tab;
            settle(h);
            assert!(h.state().view.area.is_some(), "смена вкладки сохраняет область");
            if tab == SideTab::Charts {
                assert!(h.query_by_label("Последние 10 000 тиков").is_some());
                assert!(h.query_by_label("Недавнее").is_none());
                assert!(h.query_by_label("Вся партия").is_none());
            }
        }
        h.get_by_label("Убрать рамку").click();
        settle(h);
        assert!(h.state().region.is_none() && h.state().view.area.is_none());
    });
}

#[test]
fn спокойный_профиль_работает_на_паузе() {
    let _gpu = gpu();
    each_size(|h, _size, _tag| {
        h.state_mut().side_open = false;
        settle(h);
        let tick = h.state().view.frame.as_ref().unwrap().tick;
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
        assert!(f.edits > 0, "the repeat warns of the change");
    });
}

#[test]
fn следы_залпа_и_труп_рисуются_без_окна() {
    use life_core::creature::{Program, Programs, Strategy};
    use life_core::{CreatureGenome, Rules, Shot, World, corpse::Corpse, genome::creature::Gene};
    let _gpu = gpu();
    let mut scene = World::new(&WorldConfig {
        seed: 43,
        n_creatures: Some(0),
        rules: Rules::default().with("plant_rate", 0.0).unwrap(),
        ..Default::default()
    });
    for x in [1000.0, 1020.0, 1040.0] {
        scene.spawn(CreatureGenome::BASE.with(Gene::Size, 40.0), x, 1000.0, Some(90.0));
        scene.creatures.last_mut().unwrap().programs =
            Programs::both(Program::founder(Strategy::Standard, (5, 100), true));
    }
    scene.spawn(CreatureGenome::BASE.with(Gene::Size, 80.0), 1140.0, 1000.0, Some(130.0));
    scene.spawn(CreatureGenome::BASE, 1090.0, 1050.0, Some(65.0));
    let dead = scene.creatures.pop().unwrap();
    scene.corpses.push(Corpse::from_creature(&dead, 0));
    for from in [(1000.0, 1000.0), (1020.0, 1000.0), (1040.0, 1000.0)] {
        scene.shots.push(Shot { from, to: (1140.0, 1000.0), tick: 0 });
    }
    for (size, tag) in [(SMALL, "960x600"), (NORMAL, "1600x900")] {
        let mut h = harness(size);
        h.state_mut().side_open = false;
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
        if let Some(cam) = &mut h.state_mut().view.camera {
            let (sx, sy) = cam.to_screen(1070.0, 1020.0);
            cam.zoom_at(sx, sy, 5.0);
            cam.center_on(1070.0, 1020.0);
        }
        h.step();
        shot(&mut h, &format!("залп-и-труп-{tag}"));
    }
}

/// Near, an eater stretches its proboscis to the food: a herbivore to a plant, a scavenger to a
/// rotten corpse on the bottom, a carnivore to a fresh one. The rim is the diet's colour. The
/// frame carries this in `meta` (the «eats» bit, the direction and the length); the picture
/// itself is visible in the image with LIFEGAME_SHOTS.
#[test]
fn хоботок_тянется_к_еде_вблизи() {
    use life_core::{CreatureGenome, Rules, World, corpse::Corpse, genome::creature::Gene, plant::Plant};
    let _gpu = gpu();
    let mut w = World::new(&WorldConfig {
        seed: 3,
        n_creatures: Some(0),
        rules: Rules::default().with("plant_rate", 0.0).unwrap(),
        ..Default::default()
    });
    w.tick = 700;
    let eaters = [(0.0, 1000.0), (SCAVENGER, 1300.0), (CARNIVORE, 1600.0)];
    for (diet, y) in eaters {
        w.spawn(CreatureGenome::BASE.with(Gene::Diet, diet), 1000.0, y, Some(40.0));
        w.creatures.last_mut().unwrap().reproduction_wait = 10_000;
    }
    w.plants.push(Plant::at(1100.0, 1000.0));
    for (y, born) in [(1300.0, 0), (1600.0, 699)] {
        let mut corpse = Corpse::from_creature(&w.creatures[0], born);
        (corpse.owner, corpse.x, corpse.y, corpse.y0, corpse.bottom) = (900 + born, 1110.0, y, y, y);
        (corpse.initial, corpse.remaining) = (400.0, 400.0);
        w.corpses.push(corpse);
    }
    for _ in 0..9 {
        w.step();
    }
    for v in &w.creatures {
        let meal = v.meal.expect("every one of them is eating");
        assert_eq!(meal.tick, w.tick, "{:?} bites on this tick", v.pheno.diet);
        assert!(
            (meal.x - v.x).hypot(meal.y - v.y) > v.pheno.half,
            "{:?}: the food lies beside the body",
            v.pheno.diet
        );
    }

    for (size, tag) in [(SMALL, "960x600"), (NORMAL, "1600x900")] {
        let mut h = harness(size);
        h.state_mut().side_open = false;
        let generation = h.state().view.frame.as_ref().unwrap().world_gen;
        h.state_mut().sim.send(Command::TestWorld(Box::new(w.clone())));
        for _ in 0..100 {
            h.step();
            if h.state().view.frame.as_ref().is_some_and(|f| f.world_gen > generation) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if let Some(cam) = &mut h.state_mut().view.camera {
            let (sx, sy) = cam.to_screen(1050.0, 1300.0);
            cam.zoom_at(sx, sy, 6.0);
            cam.center_on(1050.0, 1300.0);
        }
        settle(&mut h);
        let creatures: Vec<_> = h
            .state()
            .view
            .instances()
            .iter()
            .filter(|i| (i.meta >> 16) & 3 == crate::motion::KIND_CREATURE)
            .copied()
            .collect();
        assert_eq!(creatures.len(), 3, "{tag}");
        for i in &creatures {
            assert!(i.meta & crate::motion::FEEDING != 0, "{tag}: the eating bit is set");
            assert!(i.meta >> 28 > 0, "{tag}: the proboscis has a length");
            // the food lies east of each of them: the direction is near 0 (or near a full turn)
            let dir = (i.meta >> 21) & 127;
            assert!(!(8..=120).contains(&dir), "{tag}: the proboscis points at the food, {dir}/128");
        }
        let diets: Vec<u32> = creatures.iter().map(|i| (i.meta >> 12) & 3).collect();
        assert_eq!(diets, [0, 2, 3], "{tag}: the diet travels to the shader for the rim");
        shot(&mut h, &format!("хоботок-{tag}"));
    }
}

/// When zooming out the bodies become two-pixel squares, and switching the render off stops
/// collecting all the world's layers, keeping the card and the statistics.
#[test]
fn режимы_рендера_и_размер_трупа_без_окна() {
    use life_core::{CreatureGenome, Rules, World, corpse::Corpse, genome::creature::Gene};

    let _gpu = gpu();
    let cfg = WorldConfig {
        seed: 44,
        scale: 100.0,
        n_creatures: Some(0),
        rules: Rules::default().with("plant_rate", 0.0).unwrap(),
        ..Default::default()
    };
    let mut scene = World::new(&cfg);
    let body = CreatureGenome::BASE.with(Gene::Size, 80.0);
    let live = scene.spawn(body, 29_900.0, 20_000.0, Some(100.0));
    scene.spawn(body, 30_100.0, 20_000.0, Some(100.0));
    let dead = scene.creatures.pop().unwrap();
    scene.corpses.push(Corpse::from_creature(&dead, 0));

    for (size, tag) in [(SMALL, "960x600"), (NORMAL, "1600x900")] {
        let mut h = harness_with(size, cfg.clone());
        h.state_mut().side_open = false;
        let generation = h.state().view.frame.as_ref().unwrap().world_gen;
        h.state_mut().sim.send(Command::TestWorld(Box::new(scene.clone())));
        for _ in 0..100 {
            h.step();
            if h.state().view.frame.as_ref().is_some_and(|f| f.world_gen > generation && f.dots)
                && h.state().view.camera.is_some()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let f = h.state().view.frame.as_ref().unwrap();
        assert!(f.world_gen > generation && f.dots, "{tag}: дальний масштаб — квадраты");
        assert!(!h.state().view.instances().is_empty());
        assert!(h.state().view.instances().iter().all(|i| i.meta & crate::motion::DOT_BIT != 0));
        shot(&mut h, &format!("рендер-квадраты-{tag}"));

        {
            let cam = h.state_mut().view.camera.as_mut().unwrap();
            cam.zoom = 1.25;
            cam.center_on(30_000.0, 20_000.0);
        }
        for _ in 0..100 {
            h.step();
            if h.state().view.frame.as_ref().is_some_and(|f| !f.dots)
                && h.state().view.instances().iter().any(|i| i.meta & crate::motion::DOT_BIT == 0)
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!h.state().view.frame.as_ref().unwrap().dots, "{tag}: вблизи вернулись тела");
        std::thread::sleep(std::time::Duration::from_millis(750));
        settle(&mut h);
        let image = h.render().expect("снимок трупа");
        let cam = h.state().view.camera.as_ref().unwrap();
        let corpse = &h.state().view.frame.as_ref().unwrap().corpses[0];
        let (x, y) = cam.to_screen(corpse.x, corpse.y);
        let radius = corpse.size * 0.5 * cam.zoom;
        let scale = image.width() as f64 / size.x as f64;
        let sample = |offset: f64| {
            let px = ((x + offset) * scale).round() as u32;
            let py = (y * scale).round() as u32;
            assert!(px < image.width() && py < image.height());
            image.get_pixel(px, py).0
        };
        let inside = sample(radius * 0.7);
        let outside = sample(radius * 1.15);
        assert!(
            inside[0] > 70 && inside[0] > inside[2],
            "{tag}: труп виден внутри 70% радиуса тела: {inside:?}"
        );
        assert!(outside[0] < 70, "{tag}: вне тела трупа должен быть фон: {outside:?}");
        let (live_x, live_y) = cam.to_screen(29_900.0, 20_000.0);
        let ring_x = ((live_x + radius + 5.0) * scale).round() as u32;
        let ring_y = (live_y * scale).round() as u32;
        let without_selection = image.get_pixel(ring_x, ring_y).0;
        assert!(without_selection[0] < 80, "{tag}: массового кольца контакта нет");
        shot(&mut h, &format!("рендер-тело-и-труп-{tag}"));

        let f = h.state().view.frame.as_ref().unwrap();
        let (world_gen, frame) = (f.world_gen, f.number);
        let pick = Command::Pick { x: 29_900.0, y: 20_000.0, radius: 0.0, world_gen, frame, k: 1.0 };
        h.state_mut().sim.send(pick);
        for _ in 0..100 {
            h.step();
            if h.state().view.frame.as_ref().and_then(|f| f.selected).is_some_and(|s| s.id == live) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(h.state().view.frame.as_ref().unwrap().selected.unwrap().id, live);
        settle(&mut h);
        let selected_image = h.render().expect("снимок выбранного существа");
        let selected_ring = selected_image.get_pixel(ring_x, ring_y).0;
        assert!(
            selected_ring[0] > 140 && selected_ring[1] > 100 && selected_ring[2] < 110,
            "{tag}: у выбранного существа кольцо контакта: {selected_ring:?}"
        );
        shot(&mut h, &format!("рендер-выбранный-контакт-{tag}"));

        h.state_mut().render_world = false;
        h.state_mut().sim.send(Command::RenderWorld(false));
        for _ in 0..100 {
            h.step();
            if h.state().view.frame.as_ref().is_some_and(|f| !f.render_world) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let f = h.state().view.frame.as_ref().unwrap();
        assert!(!f.render_world && !f.dots && f.selected.is_some());
        assert!(h.state().view.instances().is_empty());
        assert!(f.corpses.is_empty() && f.shots.is_empty());
        assert!(f.density.is_none() && f.minimap.is_none());
        h.state_mut().side_open = true;
        h.state_mut().side_tab = SideTab::Creature;
        settle(&mut h);
        assert!(h.query_by_label("Снять выбор").is_some(), "{tag}: карточка работает без рендера");
        shot(&mut h, &format!("рендер-выкл-{tag}"));

        // An empty background stays an interactive area: planting works even without the world's picture.
        let before = h.state().view.frame.as_ref().unwrap().creatures;
        h.state_mut().tool = Tool::Spawn;
        let pos = Pos2::new(size.x * 0.35, size.y * 0.5);
        h.hover_at(pos);
        for pressed in [true, false] {
            h.event(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
        for _ in 0..100 {
            h.step();
            if h.state().view.frame.as_ref().is_some_and(|f| f.creatures == before + 1) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            h.state().view.frame.as_ref().unwrap().creatures,
            before + 1,
            "{tag}: подсадка без рендера"
        );
        h.state_mut().tool = Tool::Select;

        h.state_mut().render_world = true;
        h.state_mut().sim.send(Command::RenderWorld(true));
        for _ in 0..100 {
            h.step();
            if h.state().view.frame.as_ref().is_some_and(|f| f.render_world && !f.corpses.is_empty()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let f = h.state().view.frame.as_ref().unwrap();
        assert!(f.render_world && !f.dots && f.selected.is_some());
        assert_eq!(f.corpses.len(), 1);
    }
}

/// The selected creature's behaviour window: its template and a full mutated program of
/// `MAX_BLOCKS` blocks fit the window (the chart scrolls) and overlap no widget, on either tab
/// (the juvenile and the adult track); Esc closes it and B opens it again.
#[test]
fn поведение_выбранного_помещается_в_окно() {
    use life_core::creature::program::MAX_BLOCKS;
    use life_core::creature::{ADULT, JUVENILE, Program};
    let _gpu = gpu();
    each_size(|h, size, tag| {
        let mut w = life_core::World::new(&WorldConfig { seed: 7, ..Default::default() });
        let mut rng = life_core::rng::Rng::new(5);
        let mut full = Program::STANDARD;
        for _ in 0..10_000 {
            if full.blocks().len() == MAX_BLOCKS {
                break;
            }
            // the mutations do not grow a program by themselves: keep the ones that do not shrink it
            let before = full;
            full.drift(1.0, &mut rng);
            full.mutate_with(1.0, Some(&Program::LURKER), &mut rng);
            if full.blocks().len() < before.blocks().len() {
                full = before;
            }
        }
        assert_eq!(full.blocks().len(), MAX_BLOCKS);
        // a founder is grown: it lives by the adult track, the mutated one
        w.creatures[0].programs = [Program::LURKER, full].into();
        // a mode on for a while: the header lists it
        w.creatures[0].mind.modes[1] = 1000;
        let cases = [(w.creatures[1].id, "шаблон"), (w.creatures[0].id, "мутант")];
        let generation = h.state().view.frame.as_ref().unwrap().world_gen;
        h.state_mut().sim.send(Command::TestWorld(Box::new(w)));
        // one tick, so a block has decided
        h.state_mut().sim.send(Command::Step);
        for _ in 0..100 {
            h.step();
            if h.state().view.frame.as_ref().is_some_and(|f| f.world_gen > generation && f.tick >= 1) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        h.state_mut().side_open = true;
        h.state_mut().side_tab = SideTab::Creature;
        for (id, name) in cases {
            h.state_mut().sim.send(Command::Select(Some(id)));
            for _ in 0..100 {
                h.step();
                if h.state().view.frame.as_ref().is_some_and(|f| f.selected.is_some_and(|s| s.id == id)) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let s = h.state().view.frame.as_ref().and_then(|f| f.selected).expect("существо выбрано");
            assert_eq!(s.id, id);
            assert_eq!(s.stage, ADULT, "{name}: a founder is grown");
            let blocks = if name == "мутант" { MAX_BLOCKS } else { Program::STANDARD.blocks().len() };
            assert_eq!(s.programs[s.stage].blocks().len(), blocks);
            h.get_by_label("Поведение (B)").click();
            settle(h);
            assert!(h.state().behaviour_open, "{name}, {tag}: the card's button opens it");
            let window = Rect::from_min_size(Pos2::ZERO, size).expand(0.5);
            let top = h.get_by_label("Графики").rect();
            let bottom = h.get_by_label("Выбор").rect().top();
            let panel =
                Rect::from_min_max(Pos2::new(top.left() - 12.0, top.top()), Pos2::new(size.x, bottom));
            // the adult tab, the one it lives by, opens first; then the juvenile one
            for (stage, track) in [(ADULT, "взрослая ●"), (JUVENILE, "детская")] {
                if stage == JUVENILE {
                    h.get_by_label(track).click();
                    settle(h);
                }
                for text in ["Поведение №", "Шаблон:", "Мутаций от шаблона", "Режимы:", "Каждый тик", track]
                {
                    let r = h.get_by_label_contains(text).rect();
                    assert!(window.contains_rect(r), "{name}, {tag}: «{text}» out of the window: {r:?}");
                }
                check_layout(h, size, &format!("поведение, {name}, {track}, {tag}"), Some(panel));
                shot(
                    h,
                    &format!(
                        "поведение-{name}-{}-{tag}",
                        if stage == ADULT { "взрослая" } else { "детская" }
                    ),
                );
            }
            h.get_by_label("Поведение (B)").click();
            settle(h);
            assert!(!h.state().behaviour_open, "{name}, {tag}: the button closes it");
        }
        h.key_press(eframe::egui::Key::B);
        settle(h);
        assert!(h.state().behaviour_open, "{tag}: B opens it");
        h.key_press(eframe::egui::Key::Escape);
        settle(h);
        assert!(!h.state().behaviour_open, "{tag}: Esc closes it before the menu");
        assert_eq!(h.state().screen, Screen::Game);
    });
}

// ── A gallery of states the tests above do not picture ─────────────────────────────────────
// These only take pictures (with LIFEGAME_SHOTS, which CI sets and uploads as `ui-shots`): the
// user's own world, the endings, windows over the open side panel, a bigger interface scale.
// Without LIFEGAME_SHOTS they return at once.

fn shots_wanted() -> bool {
    std::env::var_os("LIFEGAME_SHOTS").is_some()
}

/// Step the window until `done` holds, a bounded number of times.
fn wait_for(h: &mut Harness<'static, LifeApp>, done: impl Fn(&LifeApp) -> bool) {
    for _ in 0..200 {
        h.step();
        if done(h.state()) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Select the biggest creature drawn in the frame; whether one was selected.
fn pick_biggest(h: &mut Harness<'static, LifeApp>) -> bool {
    let Some(f) = h.state().view.frame.as_ref() else { return false };
    let (world_gen, frame, origin) = (f.world_gen, f.number, f.origin);
    let Some((x, y)) = h
        .state()
        .view
        .instances()
        .iter()
        .filter(|v| (v.meta >> 16) & 3 == crate::motion::KIND_CREATURE)
        .max_by(|a, b| a.r.total_cmp(&b.r))
        .map(|i| (origin.0 + i.x as f64, origin.1 + i.y as f64))
    else {
        return false;
    };
    h.state_mut().sim.send(Command::Pick { x, y, radius: 1.0, world_gen, frame, k: 1.0 });
    wait_for(h, |s| s.view.frame.as_ref().is_some_and(|f| f.selected.is_some()));
    h.state().view.frame.as_ref().is_some_and(|f| f.selected.is_some())
}

/// The user's world: ×20, 2:1, the game's rules and founders 55/25/10/10, as they play it
/// (CLAUDE.md, «The game»). Whole, close up with a creature selected, at the deepest zoom.
#[test]
fn gallery_the_users_world() {
    if !shots_wanted() {
        return;
    }
    let _gpu = gpu();
    let mut settings = crate::settings::Settings::default();
    for (key, share) in
        [(Key::Herbivores, 55.0), (Key::Omnivores, 25.0), (Key::Scavengers, 10.0), (Key::Carnivores, 10.0)]
    {
        settings.set(key, share);
    }
    let cfg = settings.world_config(7).expect("the game's defaults make a world");
    for (size, tag) in [(SMALL, "960x600"), (NORMAL, "1600x900")] {
        let mut h = harness_with(size, cfg.clone());
        settle(&mut h);
        shot(&mut h, &format!("галерея-мир-игрока-весь-{tag}"));
        if pick_biggest(&mut h) {
            h.state_mut().side_tab = SideTab::Creature;
            settle(&mut h);
            shot(&mut h, &format!("галерея-мир-игрока-выбран-издали-{tag}"));
            let (x, y) = {
                let s = h.state().view.frame.as_ref().and_then(|f| f.selected).expect("selected");
                (s.x, s.y)
            };
            for (zoom, name) in [(0.3, "вблизи"), (crate::camera::MAX_ZOOM, "максимум")] {
                if let Some(cam) = h.state_mut().view.camera.as_mut() {
                    cam.zoom = zoom;
                    cam.center_on(x, y);
                }
                for _ in 0..60 {
                    h.step();
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                shot(&mut h, &format!("галерея-мир-игрока-выбран-{name}-{tag}"));
            }
        }
        // a diet's hint: hover its row on the panel
        h.state_mut().side_tab = SideTab::Charts;
        settle(&mut h);
        if let Some(row) = h.query_by_label("мясоеды").map(|n| n.rect()) {
            h.hover_at(row.center());
            for _ in 0..60 {
                h.step();
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            shot(&mut h, &format!("галерея-подсказка-питания-{tag}"));
        }
    }
}

/// Both endings: a world that died out and one that exploded.
#[test]
fn gallery_the_endings() {
    use life_core::{CreatureGenome, Rules, World};
    if !shots_wanted() {
        return;
    }
    let _gpu = gpu();
    let empty = WorldConfig {
        seed: 9,
        n_creatures: Some(0),
        rules: Rules::default().with("plant_rate", 0.0).unwrap(),
        ..Default::default()
    };
    let extinct = World::new(&empty);
    let mut crowded = World::new(&empty);
    let limit = crowded.space.per_area(crate::sim::EXPLOSION_LIMIT);
    for i in 0..limit + 50 {
        let (x, y) = (200.0 + (i % 80) as f64 * 70.0, 200.0 + (i / 80) as f64 * 70.0);
        crowded.spawn(CreatureGenome::BASE, x.min(5900.0), y.min(3900.0), Some(90.0));
    }
    for (size, tag) in [(SMALL, "960x600"), (NORMAL, "1600x900")] {
        for (world, name) in [(&extinct, "вымерли"), (&crowded, "взрыв")] {
            let mut h = harness(size);
            let generation = h.state().view.frame.as_ref().unwrap().world_gen;
            h.state_mut().sim.send(Command::TestWorld(Box::new(world.clone())));
            h.state_mut().sim.send(Command::Step);
            wait_for(&mut h, |s| {
                s.view.frame.as_ref().is_some_and(|f| f.world_gen > generation && f.status.ended.is_some())
            });
            settle(&mut h);
            shot(&mut h, &format!("галерея-финал-{name}-{tag}"));
        }
    }
}

/// The floating windows over the open side panel: the lab on every tab, the statistics.
#[test]
fn gallery_windows_over_the_side_panel() {
    if !shots_wanted() {
        return;
    }
    let _gpu = gpu();
    each_size(|h, _size, tag| {
        h.state_mut().side_open = true;
        h.state_mut().lab_open = true;
        for tab in Tab::RULES.map(|(tab, _)| tab) {
            h.state_mut().lab_tab = tab;
            settle(h);
            shot(h, &format!("галерея-лаборатория-с-панелью-{tab:?}-{tag}"));
        }
        h.state_mut().lab_open = false;
        h.state_mut().stats_open = true;
        for tab in [StatsTab::Energy, StatsTab::Where] {
            h.state_mut().stats_tab = tab;
            settle(h);
            shot(h, &format!("галерея-статистика-с-панелью-{tab:?}-{tag}"));
        }
        h.state_mut().stats_open = false;
    });
}

/// A middle window size and an interface scale of 150% on a usual one.
#[test]
fn gallery_other_sizes_and_scale() {
    if !shots_wanted() {
        return;
    }
    let _gpu = gpu();
    for (size, scale, tag) in [(Vec2::new(1280.0, 720.0), 0.0, "1280x720"), (NORMAL, 1.5, "1600x900-150")] {
        let mut h = harness(size);
        h.state_mut().settings.ui_scale = scale;
        settle(&mut h);
        shot(&mut h, &format!("галерея-игра-{tag}"));
        h.state_mut().screen = Screen::Menu;
        settle(&mut h);
        shot(&mut h, &format!("галерея-меню-{tag}"));
        h.state_mut().screen = Screen::Setup;
        for tab in [Tab::World, Tab::Body] {
            h.state_mut().setup_tab = tab;
            settle(&mut h);
            shot(&mut h, &format!("галерея-новый-мир-{tab:?}-{tag}"));
        }
        h.state_mut().screen = Screen::Menu;
        h.state_mut().help_open = true;
        settle(&mut h);
        shot(&mut h, &format!("галерея-справка-{tag}"));
    }
}

/// So many plants in sight that the world turns into a density map.
#[test]
fn gallery_the_density_map() {
    use life_core::{Rules, World, plant::Plant};
    if !shots_wanted() {
        return;
    }
    let _gpu = gpu();
    let cfg = WorldConfig {
        seed: 11,
        scale: 200.0,
        n_creatures: Some(0),
        rules: Rules::default().with("plant_rate", 0.0).unwrap(),
        ..Default::default()
    };
    let mut scene = World::new(&cfg);
    let (w, hh) = (scene.space.width, scene.space.height);
    let mut rng = life_core::rng::Rng::new(3);
    for _ in 0..crate::frame::MAX_INSTANCES + 20_000 {
        // denser near the surface, as the default profile grows them
        let (x, y) = (rng.random() * w, rng.random().powi(2) * hh);
        scene.plants.push(Plant::at(x, y));
    }
    for (size, tag) in [(SMALL, "960x600"), (NORMAL, "1600x900")] {
        let mut h = harness_with(size, WorldConfig { scale: 200.0, ..cfg.clone() });
        let generation = h.state().view.frame.as_ref().unwrap().world_gen;
        h.state_mut().sim.send(Command::TestWorld(Box::new(scene.clone())));
        wait_for(&mut h, |s| {
            s.view.frame.as_ref().is_some_and(|f| f.world_gen > generation && f.density.is_some())
        });
        settle(&mut h);
        shot(&mut h, &format!("галерея-карта-плотности-{tag}"));
    }
}
