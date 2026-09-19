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
            check_layout(h, size, &format!("статистика, {tab:?}, {tag}"), None);
            shot(h, &format!("статистика-{tab:?}-{tag}"));
        }
        h.state_mut().clear_region();
        settle(h);
        assert!(h.state().region.is_none() && h.state().view.area.is_none());
    });
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
