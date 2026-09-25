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
use eframe::egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use life_core::flora::Profile;
use life_core::{Shape, World, WorldConfig};

use crate::app::{LifeApp, Screen, SideTab, Tool};
use crate::frame::{Frame, Instance};
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

/// Capture the actual game renderer with a plant-only world before and after
/// clearing one depth band. Run explicitly with TINYLIFE_SHOTS set.
#[test]
#[ignore]
fn plant_depth_distribution_without_creatures() {
    let _gpu = gpu();
    use life_core::config::{PLANT_DEPTH_DECAY, PLANT_MAX, PLANT_RADIUS, PLANT_TOP_MARGIN_PCT};
    use life_core::flora::DEPTH_BANDS;

    let counts = |w: &World| {
        let mut bands = [0usize; DEPTH_BANDS];
        for p in &w.plants {
            let i = ((p.y / w.space.height * DEPTH_BANDS as f64) as usize).min(DEPTH_BANDS - 1);
            bands[i] += 1;
        }
        bands
    };
    let cfg = WorldConfig { seed: 3, n_creatures: Some(0), ..Default::default() };
    let mut world = World::new(&cfg);
    for _ in 0..3000 {
        world.step();
    }
    let full = counts(&world);
    let lo = PLANT_TOP_MARGIN_PCT / 100.0 + PLANT_RADIUS / world.space.height;
    let hi = 1.0 - PLANT_RADIUS / world.space.height;
    let (e_lo, e_hi) = ((-PLANT_DEPTH_DECAY * lo).exp(), (-PLANT_DEPTH_DECAY * hi).exp());
    let mut expected = [0; DEPTH_BANDS];
    let mut previous = 0;
    for (i, band) in expected.iter_mut().enumerate() {
        let edge = ((i + 1) as f64 / DEPTH_BANDS as f64).clamp(lo, hi);
        let cumulative =
            (PLANT_MAX as f64 * (e_lo - (-PLANT_DEPTH_DECAY * edge).exp()) / (e_lo - e_hi)).round() as usize;
        *band = cumulative - previous;
        previous = cumulative;
    }
    assert_eq!(full, expected, "the full world follows the integrated exponential profile");

    for (size, tag) in [(SMALL, "960x600"), (NORMAL, "1600x900")] {
        let mut h = Harness::builder()
            .with_size(size)
            .wgpu()
            .build_eframe(|cc| LifeApp::new(cc, Some(cfg.clone()), None));
        h.state_mut().sim.send(Command::SetPaused(true));
        h.state_mut().side_open = false;
        for _ in 0..100 {
            h.step();
            if h.state().view.frame.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(h.state().view.frame.is_some());
        h.state_mut().freeze_sim_frame = true;

        let mut show = |world: &World, stage: &str| {
            let mut instances = Vec::new();
            assert!(crate::frame::dots_colored(
                world,
                (0.0, 0.0, world.space.width, world.space.height),
                &mut instances,
                true,
            ));
            let frame = Frame {
                world_gen: 1,
                scale: 1.0,
                rules: world.rules.clone(),
                tick: world.tick,
                plants: world.plants.len(),
                world_w: world.space.width,
                world_h: world.space.height,
                status: crate::frame::Status { paused: true, ..Default::default() },
                render_world: true,
                dots: true,
                instances,
                ..Default::default()
            };
            let app = h.state_mut();
            app.view.accept(&eframe::egui::Context::default(), &app.sim, frame);
            h.step();
            assert_eq!(h.state().view.frame.as_ref().map(|f| f.tick), Some(world.tick));
            shot(&mut h, &format!("plants-{stage}-{tag}"));
        };

        show(&world, "full");
        let mut cleared = world.clone();
        cleared.plants.retain(|p| !(0.2..0.3).contains(&(p.y / cleared.space.height)));
        let empty = counts(&cleared);
        assert_eq!(empty[2], 0);
        assert!(empty.iter().enumerate().all(|(i, n)| i == 2 || *n == full[i]));
        show(&cleared, "cleared");
        for _ in 0..150 {
            cleared.step();
        }
        let growing = counts(&cleared);
        assert!(growing[2] > 0 && growing[2] < full[2]);
        assert!(growing.iter().enumerate().all(|(i, n)| i == 2 || *n == full[i]));
        show(&cleared, "regrowing");
        for _ in 0..1000 {
            cleared.step();
        }
        let restored = counts(&cleared);
        assert_eq!(restored, full, "the cleared band returns to its exact old capacity");
        show(&cleared, "restored");

        if tag == "1600x900"
            && let Ok(dir) = std::env::var("TINYLIFE_SHOTS")
        {
            let mut table = String::from(
                "# Plant depth distribution (no creatures)\n\nBaseline tick 3000. Removed all plants at 20–30% depth; partial recovery after 150 ticks, full recovery after another 1000 ticks.\n\n| Depth | Exponential target | Full | Cleared | +150 ticks | +1150 ticks |\n| --- | ---: | ---: | ---: | ---: | ---: |\n",
            );
            for i in 0..DEPTH_BANDS {
                table.push_str(&format!(
                    "| {}–{}% | {} | {} | {} | {} | {} |\n",
                    i * 10,
                    (i + 1) * 10,
                    expected[i],
                    full[i],
                    empty[i],
                    growing[i],
                    restored[i]
                ));
            }
            table.push_str(&format!(
                "| Total | {} | {} | {} | {} | {} |\n",
                expected.iter().sum::<usize>(),
                full.iter().sum::<usize>(),
                empty.iter().sum::<usize>(),
                growing.iter().sum::<usize>(),
                restored.iter().sum::<usize>()
            ));
            std::fs::write(format!("{dir}/bands.md"), table).expect("depth counts saved");
        }
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
fn лаборатория_сбрасывает_отмеченные_цены_и_показывается_без_окна() {
    let _gpu = gpu();
    each_size(|h, size, tag| {
        h.state_mut().lab_open = true;
        h.state_mut().lab_tab = Tab::Lab;
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
            if tab != StatsTab::Region {
                assert!(h.query_by_label("Последние 10 000 тиков").is_some());
            }
            check_layout(h, size, &format!("статистика, {tab:?}, {tag}"), None);
            shot(h, &format!("статистика-{tab:?}-{tag}"));
        }
        h.state_mut().stats_open = false;
        h.state_mut().side_open = true;
        for tab in [SideTab::Charts, SideTab::Log, SideTab::Creature] {
            h.state_mut().side_tab = tab;
            settle(h);
            assert!(h.state().view.area.is_some(), "смена вкладки сохраняет область");
            if tab == SideTab::Charts {
                assert!(h.query_by_label("Последние 10 000 тиков").is_some());
                let battle = if h.state().view.frame.as_ref().unwrap().rules.cannibals() {
                    "Бои/каннибализм: включены"
                } else {
                    "Бои/каннибализм: выключены"
                };
                assert!(h.query_by_label(battle).is_some());
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
fn flock_scenes_and_card_without_a_window() {
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
    let (mut alarm, mut back) = (false, false);
    for _ in 0..200 {
        w.step();
        if !alarm && w.creatures.iter().any(|v| v.mind.social.activity == Activity::Alarm) {
            scenes.push(("тревога", w.clone()));
            alarm = true;
        }
        // after the alarm the members walk back into their circle
        if alarm
            && !back
            && w.creatures.iter().all(|v| v.mind.social.activity != Activity::Alarm)
            && w.creatures.iter().any(|v| v.mind.social.activity == Activity::Gathering)
        {
            scenes.push(("возврат", w.clone()));
            back = true;
        }
    }
    assert!(alarm && back, "after the threat is gone the flock returns to its circle");
    // two hard flocks side by side fight for room
    let mut w = fixture();
    w.creatures.clear();
    let hard = CreatureGenome::BASE.with(Gene::Territoriality, 2.0);
    for (i, x) in [2500.0, 2540.0, 2580.0, 2620.0, 3380.0, 3420.0, 3460.0, 3500.0].into_iter().enumerate() {
        w.spawn(hard, x, 1970.0 + (i % 2) as f64 * 60.0, Some(65.0));
    }
    let n = w.creatures.len();
    let (left, right) = (w.creatures[n - 8].flock, w.creatures[n - 4].flock);
    for (k, v) in w.creatures[n - 8..].iter_mut().enumerate() {
        v.flock = if k < 4 { left } else { right };
        v.reproduction_wait = 10000;
    }
    life_core::flock::update(&mut w.flocks, &mut w.creatures, &w.space, 42, false);
    let sides: std::collections::BTreeMap<u64, usize> = [(left, 4), (right, 4)].into();
    w.battles.active.push(life_core::battle::Battle { id: 0, since: w.tick, sides });
    for tag in [left, right] {
        w.flocks.get_mut(&tag).unwrap().battle = Some(0);
    }
    scenes.push(("бой-стай", w));
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
        // the circle is the territory: a click near its edge, away from bodies, picks the flock
        let area = h.state().view.frame.as_ref().unwrap().flock_areas[0].clone();
        let outside_group = (0..72)
            .map(|i| {
                let angle = i as f64 * std::f64::consts::TAU / 72.0;
                (area.x + area.radius * 0.9 * angle.cos(), area.y + area.radius * 0.9 * angle.sin())
            })
            .find(|&(x, y)| territory.pick(x, y, 0.0).is_none())
            .expect("a part of the circle without bodies");
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

/// При отдалении тела становятся двухпиксельными квадратами, а выключение рендера
/// прекращает сбор всех слоёв мира, сохраняя карточку и статистику.
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

        h.state_mut().sim.send(Command::Pick { x: 29_900.0, y: 20_000.0, radius: 0.0 });
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
        assert!(f.corpses.is_empty() && f.shots.is_empty() && f.flock_areas.is_empty());
        assert!(f.density.is_none() && f.minimap.is_none());
        h.state_mut().side_open = true;
        h.state_mut().side_tab = SideTab::Creature;
        settle(&mut h);
        assert!(h.query_by_label("Снять выбор").is_some(), "{tag}: карточка работает без рендера");
        shot(&mut h, &format!("рендер-выкл-{tag}"));

        // Пустой фон остаётся интерактивной областью: подсадка работает и без рисунка мира.
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
