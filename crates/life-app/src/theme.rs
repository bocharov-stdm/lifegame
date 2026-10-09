//! The game's look: the one palette, the fonts, the sizes and the shapes of widgets. Every colour
//! of the interface, the charts and the world is here (the shader gets the diets' through its
//! uniforms): a dark deep-water ground, panels in a soft vertical gradient, thin lit edges and
//! one cold accent for the interface's state. The diets' colours stay apart for colour-blind eyes
//! (`tests::the_diets_stay_apart_for_colour_blind_eyes`).

use std::sync::Arc;

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Shadow, Stroke, TextStyle};

// ── surfaces ────────────────────────────────────────────────────────────────
/// Behind everything, outside the world.
pub const BG: Color32 = Color32::from_rgb(6, 10, 17);
/// Panels and windows: a vertical gradient from the top to the bottom colour.
pub const PANEL: Color32 = Color32::from_rgb(11, 18, 29);
pub const PANEL_TOP: Color32 = Color32::from_rgb(15, 24, 38);
pub const PANEL_BOTTOM: Color32 = Color32::from_rgb(9, 15, 25);
/// A raised surface: an input, a button at rest, a card in a panel.
pub const CARD: Color32 = Color32::from_rgb(16, 27, 41);
pub const CARD_HOVER: Color32 = Color32::from_rgb(22, 38, 56);
/// Hairlines and borders.
pub const LINE: Color32 = Color32::from_rgb(30, 52, 72);

// ── text ────────────────────────────────────────────────────────────────────
pub const TEXT: Color32 = Color32::from_rgb(222, 234, 244);
/// Secondary text: 7.9:1 on `PANEL`, 7.3:1 on `CARD`.
pub const MUTED: Color32 = Color32::from_rgb(150, 170, 190);
/// The least of the texts (a footnote), still 5:1 on `BG`.
pub const FAINT: Color32 = Color32::from_rgb(118, 138, 158);

// ── the interface's state ───────────────────────────────────────────────────
/// The cold accent: what is selected, on, followed, the main action. Never a diet's colour.
pub const ACCENT: Color32 = Color32::from_rgb(70, 200, 255);
/// The far end of the main button's gradient and of lit lines.
pub const ACCENT_FAR: Color32 = Color32::from_rgb(110, 116, 246);
/// A pressed or chosen control's fill.
pub const ACCENT_DEEP: Color32 = Color32::from_rgb(20, 52, 72);
/// Text on the accent.
pub const ACCENT_TEXT: Color32 = Color32::from_rgb(4, 12, 20);
pub const GOOD: Color32 = Color32::from_rgb(92, 222, 170);
pub const WARN: Color32 = Color32::from_rgb(242, 184, 84);
pub const DANGER: Color32 = Color32::from_rgb(255, 92, 112);
/// The dimming behind the menu.
pub const VEIL: Color32 = Color32::from_rgba_premultiplied(3, 6, 10, 200);

// ── the world ───────────────────────────────────────────────────────────────
/// The water from the surface down to the bottom.
pub const WORLD_TOP: [u8; 3] = [12, 32, 50];
pub const WORLD_BOTTOM: [u8; 3] = [3, 8, 16];
/// The warm water just above the thermocline and the cold one below it.
const WATER_WARM: [u8; 3] = [10, 30, 46];
const WATER_COLD: [u8; 3] = [5, 15, 30];

/// The water's colour at `depth` % of the world: lighter at the surface, a warm layer down to the
/// thermocline's top (`top`, %), a smooth turn to the cold water at its bottom, then darker to the
/// floor — so the layers that matter to the cold-blooded are seen.
pub fn water(depth: f64, top: f64, bottom: f64) -> [u8; 3] {
    let lerp = |a: [u8; 3], b: [u8; 3], t: f64| -> [u8; 3] {
        let t = t.clamp(0.0, 1.0);
        std::array::from_fn(|i| (a[i] as f64 + (b[i] as f64 - a[i] as f64) * t).round() as u8)
    };
    let (top, bottom) = (top.clamp(0.0, 100.0), bottom.clamp(top.clamp(0.0, 100.0), 100.0));
    if depth <= top {
        lerp(WORLD_TOP, WATER_WARM, depth / top.max(1e-9))
    } else if depth < bottom {
        let t = (depth - top) / (bottom - top);
        lerp(WATER_WARM, WATER_COLD, t * t * (3.0 - 2.0 * t))
    } else {
        lerp(WATER_COLD, WORLD_BOTTOM, (depth - bottom) / (100.0 - bottom).max(1e-9))
    }
}
/// Plants on the charts, the counters, the density map and the minimap: sage, apart from every
/// diet; the sprouts in the world are a darker green of their own.
pub const PLANT_COLOR: [u8; 3] = [144, 181, 122];
pub const SPROUT_COLOR: [u8; 3] = [72, 150, 92];
/// Creatures all together (a chart line, a counter): a neutral light, not a diet's colour.
pub const CREATURE_COLOR: [u8; 3] = [200, 210, 228];
/// A shot's trail.
pub const SHOT_COLOR: [u8; 3] = [255, 216, 122];
/// Plants' line on the population chart.
pub const PLANT_LINE: [u8; 3] = PLANT_COLOR;
/// Corpses: fresh meat a light red, rot a dark olive, bones pale — apart by lightness as well as
/// by hue, so colour-blind eyes tell them too.
pub const CORPSE_FRESH: [u8; 3] = [226, 92, 96];
pub const CORPSE_ROT: [u8; 3] = [92, 98, 58];
pub const CORPSE_BONES: [u8; 3] = [226, 218, 196];

// ── charts and diagrams ─────────────────────────────────────────────────────
/// The variants of a choice gene, in order (the strategies' templates first).
pub const VARIANT_COLORS: [Color32; 5] = [
    Color32::from_rgb(200, 210, 228),
    Color32::from_rgb(242, 184, 84),
    Color32::from_rgb(92, 222, 170),
    Color32::from_rgb(70, 200, 255),
    Color32::from_rgb(255, 92, 112),
];
/// The body's three stats on the price chart: size, speed, sight.
pub const STAT_COLORS: [Color32; 3] =
    [Color32::from_rgb(242, 184, 84), Color32::from_rgb(70, 200, 255), Color32::from_rgb(180, 150, 255)];
/// The flowchart's boxes: a setting indigo, standing still amber, any other move silver. None is
/// the accent, which lights this tick's path.
pub const FLOW_SETTING: Color32 = Color32::from_rgb(110, 116, 246);
pub const FLOW_STILL: Color32 = Color32::from_rgb(226, 180, 110);
pub const FLOW_MOVE: Color32 = Color32::from_rgb(196, 206, 222);

/// Diet colours, in `Diet` order: herbivore aquamarine, omnivore pale yellow, scavenger violet,
/// carnivore orange. Bright on the dark ground and apart under every kind of colour blindness.
pub const DIET_COLORS: [[u8; 3]; 4] = [[60, 226, 190], [255, 238, 137], [214, 128, 255], [255, 112, 48]];
/// Diet names for lists and headings.
pub const DIET_NAMES: [&str; 4] = ["травоядные", "всеядные", "падальщики", "мясоеды"];
/// What each diet eats, in plain words, for hover hints.
pub const DIET_HINTS: [&str; 4] = [
    "Едят растения и усваивают их лучше всех; свежее мясо — только с голодухи, и то плохо. Здоровья чуть больше.",
    "Едят всё подряд — растения, свежее мясо, гниль, — но всё усваивают похуже специалистов.",
    "Едят гниль и кости, что опускаются на дно (кости — только они), свежее мясо — с голодухи. \
     Чуют трупы издалека. Растения едят плохо, в детстве лучше.",
    "Охотятся на тех, кто заметно мельче, и едят свежее мясо. Бьют сильнее всех, бегать им дешевле. \
     Растения едят плохо, в детстве лучше.",
];

// ── sizes ───────────────────────────────────────────────────────────────────
/// Text sizes: a screen's title, a section's heading, ordinary text, a caption.
pub const TITLE: f32 = 26.0;
pub const HEADING: f32 = 17.0;
pub const BODY: f32 = 14.0;
pub const SMALL: f32 = 12.0;
/// Spacing steps.
pub const GAP: f32 = 8.0;
pub const GAP_WIDE: f32 = 16.0;
/// Corners: widgets, windows.
pub const RADIUS: u8 = 7;
pub const RADIUS_WINDOW: u8 = 10;

/// The family of headings: the semibold cut of the text face.
pub fn strong_family() -> FontFamily {
    FontFamily::Name("strong".into())
}

/// Emphasised text — a heading, a name: the semibold cut, in the main text colour.
pub fn strong(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).family(strong_family()).color(TEXT)
}

/// A heading's text: semibold, at `size`.
pub fn heading(text: impl Into<String>, size: f32) -> egui::RichText {
    strong(text).size(size)
}

/// The frame of a side, top or bottom panel without its fill: `backdrop` paints the ground.
pub fn panel_frame(style: &egui::Style) -> egui::Frame {
    egui::Frame::side_top_panel(style).fill(Color32::TRANSPARENT)
}

pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// 12345 → «12 345»: big numbers read easier.
pub fn spaced(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('\u{202F}');
        }
        out.push(ch);
    }
    out
}

/// The fonts: Inter for text (and its semibold cut for headings), JetBrains Mono for numbers in
/// columns; both under the SIL Open Font License (`assets/fonts`). egui's own Ubuntu and Hack stay
/// behind them for the glyphs they lack (the chronicle's «→ ‒ ●»), then the emoji.
fn fonts() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.to_owned(), Arc::new(egui::FontData::from_static(bytes)));
    };
    add("inter", include_bytes!("../assets/fonts/Inter-Regular.ttf"));
    add("inter-semibold", include_bytes!("../assets/fonts/Inter-SemiBold.ttf"));
    add("jetbrains-mono", include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"));
    let defaults = |family: FontFamily| fonts.families.get(&family).cloned().unwrap_or_default();
    let (proportional, monospace) = (defaults(FontFamily::Proportional), defaults(FontFamily::Monospace));
    let with = |first: &[&str], rest: &[String]| -> Vec<String> {
        let mut list: Vec<String> = first.iter().map(|s| s.to_string()).collect();
        for name in rest.iter().cloned().chain(["Hack".to_string()]) {
            if !list.contains(&name) {
                list.push(name);
            }
        }
        list
    };
    fonts.families.insert(FontFamily::Proportional, with(&["inter"], &proportional));
    fonts.families.insert(FontFamily::Monospace, with(&["jetbrains-mono"], &monospace));
    fonts.families.insert(strong_family(), with(&["inter-semibold", "inter"], &proportional));
    fonts
}

/// Always the dark look: the world is dark, and a light panel over it hurts the eye.
pub fn apply(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = BG;
    v.faint_bg_color = CARD;
    v.code_bg_color = CARD;
    v.hyperlink_color = ACCENT;
    v.warn_fg_color = WARN;
    v.error_fg_color = DANGER;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.window_corner_radius = CornerRadius::same(RADIUS_WINDOW);
    v.menu_corner_radius = CornerRadius::same(RADIUS);
    v.window_stroke = Stroke::new(1.0, LINE);
    // a window floats in a faint cold glow rather than a heavy shadow
    v.window_shadow = Shadow { offset: [0, 6], blur: 22, spread: 0, color: ACCENT.gamma_multiply(0.10) };
    v.popup_shadow = Shadow { offset: [0, 4], blur: 14, spread: 0, color: Color32::from_black_alpha(150) };
    let radius = CornerRadius::same(RADIUS);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = PANEL;
    w.noninteractive.weak_bg_fill = PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = radius;
    w.inactive.bg_fill = CARD;
    w.inactive.weak_bg_fill = CARD;
    w.inactive.bg_stroke = Stroke::new(1.0, LINE);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = radius;
    w.hovered.bg_fill = CARD_HOVER;
    w.hovered.weak_bg_fill = CARD_HOVER;
    w.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.6));
    w.hovered.fg_stroke = Stroke::new(1.5, TEXT);
    w.hovered.corner_radius = radius;
    w.active.bg_fill = ACCENT_DEEP;
    w.active.weak_bg_fill = ACCENT_DEEP;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.5, TEXT);
    w.active.corner_radius = radius;
    w.open.bg_fill = CARD_HOVER;
    w.open.weak_bg_fill = CARD_HOVER;
    w.open.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.6));
    w.open.fg_stroke = Stroke::new(1.0, TEXT);
    w.open.corner_radius = radius;
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.all_styles_mut(|s| {
        s.text_styles = [
            (TextStyle::Heading, FontId::new(20.0, strong_family())),
            (TextStyle::Body, FontId::proportional(BODY)),
            (TextStyle::Button, FontId::proportional(BODY)),
            (TextStyle::Small, FontId::proportional(SMALL)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(GAP, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 4.0);
        s.spacing.slider_width = 220.0;
        s.spacing.interact_size.y = 24.0;
    });
}

/// A soft vertical gradient between two colours over `rect` (a mesh: egui fills its shapes with
/// one colour only).
pub fn gradient(painter: &egui::Painter, rect: egui::Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// A rounded rectangle filled from `left` to `right`: a fan from its centre over its outline,
/// each point coloured by how far across it lies.
fn rounded_gradient(rect: egui::Rect, radius: f32, left: Color32, right: Color32) -> egui::Mesh {
    let r = radius.min(rect.height() / 2.0).min(rect.width() / 2.0);
    let across = |x: f32| {
        let t = ((x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
        egui::lerp(egui::Rgba::from(left)..=egui::Rgba::from(right), t).into()
    };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.center(), across(rect.center().x));
    // the corners' centres, clockwise from the top right, and the angle each arc starts at
    let corners = [
        (egui::pos2(rect.right() - r, rect.top() + r), -90.0_f32),
        (egui::pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (egui::pos2(rect.left() + r, rect.bottom() - r), 90.0),
        (egui::pos2(rect.left() + r, rect.top() + r), 180.0),
    ];
    const STEPS: usize = 6;
    for (c, start) in corners {
        for k in 0..=STEPS {
            let a = (start + 90.0 * k as f32 / STEPS as f32).to_radians();
            let p = c + egui::vec2(a.cos(), a.sin()) * r;
            mesh.colored_vertex(p, across(p.x));
        }
    }
    let n = mesh.vertices.len() as u32;
    for i in 1..n {
        mesh.add_triangle(0, i, if i + 1 < n { i + 1 } else { 1 });
    }
    mesh
}

/// A panel's ground: the gradient over its whole area and a lit hairline along the edge that
/// faces the world (`edge`: which side). Drawn first in the panel, under its widgets.
pub fn backdrop(ui: &egui::Ui, edge: Option<egui::Align2>) {
    let rect = ui.clip_rect();
    let painter = ui.painter();
    gradient(painter, rect, PANEL_TOP, PANEL_BOTTOM);
    let lit = Stroke::new(1.0, ACCENT.gamma_multiply(0.45));
    match edge {
        Some(egui::Align2::CENTER_BOTTOM) => {
            painter.hline(rect.x_range(), rect.bottom() - 0.5, lit);
        }
        Some(egui::Align2::CENTER_TOP) => {
            painter.hline(rect.x_range(), rect.top() + 0.5, lit);
        }
        Some(egui::Align2::LEFT_CENTER) => {
            painter.vline(rect.left() + 0.5, rect.y_range(), lit);
        }
        Some(egui::Align2::RIGHT_CENTER) => {
            painter.vline(rect.right() - 0.5, rect.y_range(), lit);
        }
        _ => {}
    }
}

/// The main action's button: a cold gradient with dark text and a faint glow around it.
pub fn primary_button(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    primary_sized(ui, text, BODY)
}

/// The same at another text size (the menu's big buttons).
pub fn primary_sized(ui: &mut egui::Ui, text: impl Into<String>, size: f32) -> egui::Response {
    let under = ui.painter().add(egui::Shape::Noop);
    let label = egui::RichText::new(text).family(strong_family()).size(size).color(ACCENT_TEXT);
    let response =
        ui.add(egui::Button::new(label).fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, ACCENT)));
    let enabled = ui.is_enabled();
    let k = match (enabled, response.hovered()) {
        (false, _) => 0.35,
        (true, true) => 1.0,
        (true, false) => 0.85,
    };
    let rect = response.rect;
    let fill = rounded_gradient(rect, RADIUS as f32, ACCENT.gamma_multiply(k), ACCENT_FAR.gamma_multiply(k));
    let glow = egui::epaint::RectShape::filled(
        rect.expand(3.0),
        CornerRadius::same(RADIUS + 3),
        ACCENT.gamma_multiply(if enabled { 0.12 } else { 0.0 }),
    );
    ui.painter().set(under, egui::Shape::Vec(vec![glow.into(), egui::Shape::mesh(fill)]));
    response
}

/// An action that loses something (a restart, a reset): dark, with a red edge.
pub fn danger_button(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    let label = egui::RichText::new(text).color(DANGER);
    ui.add(egui::Button::new(label).fill(DANGER.gamma_multiply(0.12)).stroke(Stroke::new(1.0, DANGER)))
}

/// The icons of the tool rail and the top bar, drawn as lines: no font has them all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Menu,
    Play,
    Pause,
    Step,
    Slower,
    Faster,
    Select,
    Spawn,
    Area,
    Fit,
    Follow,
    Panel,
    Stats,
    Behaviour,
    Lab,
    Help,
}

/// A square button with an icon; `label` is what a hover and a screen reader say. `selected`:
/// lit with the accent, as a tool in use or a window open.
pub fn icon_button(ui: &mut egui::Ui, icon: Icon, label: &str, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(32.0, 28.0), egui::Sense::click());
    let enabled = ui.is_enabled();
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, label));
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered() && enabled;
        let (fill, edge) = match (selected, hovered) {
            (true, _) => (ACCENT_DEEP, ACCENT),
            (false, true) => (CARD_HOVER, ACCENT.gamma_multiply(0.6)),
            (false, false) => (CARD, LINE),
        };
        let painter = ui.painter();
        painter.rect(
            rect,
            CornerRadius::same(RADIUS),
            fill,
            Stroke::new(1.0, edge),
            egui::StrokeKind::Inside,
        );
        let ink = if !enabled {
            MUTED.gamma_multiply(0.5)
        } else if selected {
            ACCENT
        } else {
            TEXT
        };
        paint_icon(painter, icon, rect.shrink2(egui::vec2(9.0, 7.0)), ink);
    }
    response.on_hover_text(label)
}

/// An icon's lines inside `r`.
pub fn paint_icon(painter: &egui::Painter, icon: Icon, r: egui::Rect, ink: Color32) {
    use egui::{Shape, pos2};
    let line = Stroke::new(1.6, ink);
    let (l, t, rt, b) = (r.left(), r.top(), r.right(), r.bottom());
    let (cx, cy) = (r.center().x, r.center().y);
    let at = |x: f32, y: f32| pos2(l + (rt - l) * x, t + (b - t) * y);
    match icon {
        Icon::Menu => {
            for y in [0.15, 0.5, 0.85] {
                painter.line_segment([at(0.0, y), at(1.0, y)], line);
            }
        }
        Icon::Play => {
            painter.add(Shape::convex_polygon(
                vec![at(0.2, 0.0), at(0.9, 0.5), at(0.2, 1.0)],
                ink,
                Stroke::NONE,
            ));
        }
        Icon::Pause => {
            painter.rect_filled(egui::Rect::from_min_max(at(0.18, 0.0), at(0.4, 1.0)), 1.0, ink);
            painter.rect_filled(egui::Rect::from_min_max(at(0.6, 0.0), at(0.82, 1.0)), 1.0, ink);
        }
        Icon::Step => {
            painter.add(Shape::convex_polygon(
                vec![at(0.1, 0.0), at(0.7, 0.5), at(0.1, 1.0)],
                ink,
                Stroke::NONE,
            ));
            painter.rect_filled(egui::Rect::from_min_max(at(0.75, 0.0), at(0.92, 1.0)), 1.0, ink);
        }
        Icon::Slower => {
            painter.line_segment([at(0.15, 0.5), at(0.85, 0.5)], line);
        }
        Icon::Faster => {
            painter.line_segment([at(0.15, 0.5), at(0.85, 0.5)], line);
            painter.line_segment([pos2(cx, t + 1.0), pos2(cx, b - 1.0)], line);
        }
        Icon::Select => {
            let arrow = vec![
                at(0.2, 0.0),
                at(0.85, 0.6),
                at(0.52, 0.62),
                at(0.68, 1.0),
                at(0.56, 1.0),
                at(0.4, 0.66),
                at(0.2, 0.85),
            ];
            painter.add(Shape::closed_line(arrow, line));
        }
        Icon::Spawn => {
            painter.circle_stroke(pos2(cx, cy), (b - t) * 0.5, line);
            painter.line_segment([pos2(cx - 3.5, cy), pos2(cx + 3.5, cy)], line);
            painter.line_segment([pos2(cx, cy - 3.5), pos2(cx, cy + 3.5)], line);
        }
        Icon::Area => {
            let corners = [at(0.0, 0.0), at(1.0, 0.0), at(1.0, 1.0), at(0.0, 1.0), at(0.0, 0.0)];
            painter.extend(Shape::dashed_line(&corners, Stroke::new(1.4, ink), 3.0, 2.0));
        }
        Icon::Fit => {
            for (x, y, dx, dy) in
                [(0.0, 0.0, 1.0, 1.0), (1.0, 0.0, -1.0, 1.0), (0.0, 1.0, 1.0, -1.0), (1.0, 1.0, -1.0, -1.0)]
            {
                let c = at(x, y);
                painter.line_segment([c, c + egui::vec2(5.0 * dx, 0.0)], line);
                painter.line_segment([c, c + egui::vec2(0.0, 5.0 * dy)], line);
            }
        }
        Icon::Follow => {
            let radius = (b - t) * 0.42;
            painter.circle_stroke(pos2(cx, cy), radius, line);
            painter.circle_filled(pos2(cx, cy), 1.8, ink);
            for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                let from = pos2(cx + dx * (radius - 1.0), cy + dy * (radius - 1.0));
                painter.line_segment([from, from + egui::vec2(dx * 3.5, dy * 3.5)], line);
            }
        }
        Icon::Panel => {
            painter.rect_stroke(r, 1.5, line, egui::StrokeKind::Inside);
            painter.line_segment([at(0.62, 0.0), at(0.62, 1.0)], line);
        }
        Icon::Stats => {
            for (x, h) in [(0.1, 0.45), (0.45, 0.8), (0.8, 0.6)] {
                painter.rect_filled(
                    egui::Rect::from_min_max(at(x - 0.1, 1.0 - h), at(x + 0.12, 1.0)),
                    1.0,
                    ink,
                );
            }
        }
        Icon::Behaviour => {
            painter.rect_stroke(
                egui::Rect::from_min_max(at(0.0, 0.0), at(0.45, 0.38)),
                1.0,
                line,
                egui::StrokeKind::Inside,
            );
            painter.rect_stroke(
                egui::Rect::from_min_max(at(0.55, 0.62), at(1.0, 1.0)),
                1.0,
                line,
                egui::StrokeKind::Inside,
            );
            painter.add(Shape::line(vec![at(0.22, 0.38), at(0.22, 0.81), at(0.55, 0.81)], line));
        }
        Icon::Lab => {
            let flask = vec![
                at(0.36, 0.0),
                at(0.64, 0.0),
                at(0.64, 0.38),
                at(0.95, 1.0),
                at(0.05, 1.0),
                at(0.36, 0.38),
            ];
            painter.add(Shape::closed_line(flask, line));
            painter.line_segment([at(0.2, 0.72), at(0.8, 0.72)], Stroke::new(1.2, ink));
        }
        Icon::Help => {
            painter.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "?",
                FontId::new(15.0, strong_family()),
                ink,
            );
        }
    }
}

/// A small rounded badge with a word in it: «пауза», «отстаёт», «мир изменён».
pub fn chip(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    let label = egui::RichText::new(text).family(strong_family()).size(SMALL).color(color);
    egui::Frame::new()
        .fill(color.gamma_multiply(0.14))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.8)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| ui.label(label))
        .response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn числа_с_разрядами() {
        assert_eq!(spaced(0), "0");
        assert_eq!(spaced(999), "999");
        assert_eq!(spaced(12345), "12\u{202F}345");
        assert_eq!(spaced(1234567), "1\u{202F}234\u{202F}567");
    }

    /// sRGB to linear light.
    fn linear(c: [u8; 3]) -> [f64; 3] {
        c.map(|v| {
            let v = v as f64 / 255.0;
            if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        })
    }

    /// Linear light through a matrix, back to an sRGB byte triple.
    fn through(m: [[f64; 3]; 3], c: [u8; 3]) -> [u8; 3] {
        let l = linear(c);
        let row = |r: [f64; 3]| {
            let v = (r[0] * l[0] + r[1] * l[1] + r[2] * l[2]).clamp(0.0, 1.0);
            let s = if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
            (s * 255.0).round() as u8
        };
        [row(m[0]), row(m[1]), row(m[2])]
    }

    /// CIELAB of an sRGB colour (D65).
    fn lab(c: [u8; 3]) -> [f64; 3] {
        let l = linear(c);
        let x = (0.4124 * l[0] + 0.3576 * l[1] + 0.1805 * l[2]) / 0.95047;
        let y = 0.2126 * l[0] + 0.7152 * l[1] + 0.0722 * l[2];
        let z = (0.0193 * l[0] + 0.1192 * l[1] + 0.9505 * l[2]) / 1.08883;
        let f = |t: f64| if t > 0.008856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
        [116.0 * f(y) - 16.0, 500.0 * (f(x) - f(y)), 200.0 * (f(y) - f(z))]
    }

    fn delta_e(a: [u8; 3], b: [u8; 3]) -> f64 {
        let (a, b) = (lab(a), lab(b));
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    }

    /// WCAG contrast of two colours.
    fn contrast(a: Color32, b: Color32) -> f64 {
        let lum = |c: Color32| {
            let l = linear([c.r(), c.g(), c.b()]);
            0.2126 * l[0] + 0.7152 * l[1] + 0.0722 * l[2]
        };
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// Every pair of diets differs by ΔE ≥ 25 to normal eyes and under a full deuteranopia,
    /// protanopia and tritanopia (Machado et al., 2009): prey and hunter never look alike.
    #[test]
    fn the_diets_stay_apart_for_colour_blind_eyes() {
        const NORMAL: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        const DEUTAN: [[f64; 3]; 3] = [
            [0.367322, 0.860646, -0.227968],
            [0.280085, 0.672501, 0.047413],
            [-0.011820, 0.042940, 0.968881],
        ];
        const PROTAN: [[f64; 3]; 3] = [
            [0.152286, 1.052583, -0.204868],
            [0.114503, 0.786281, 0.099216],
            [-0.003882, -0.048116, 1.051998],
        ];
        const TRITAN: [[f64; 3]; 3] = [
            [1.255528, -0.076749, -0.178779],
            [-0.078411, 0.930809, 0.147602],
            [0.004733, 0.691367, 0.303900],
        ];
        for (eyes, m) in [("normal", NORMAL), ("deutan", DEUTAN), ("protan", PROTAN), ("tritan", TRITAN)] {
            for i in 0..4 {
                for j in i + 1..4 {
                    let d = delta_e(through(m, DIET_COLORS[i]), through(m, DIET_COLORS[j]));
                    assert!(
                        d >= 25.0,
                        "{eyes}: {} and {} only ΔE {d:.0} apart",
                        DIET_NAMES[i],
                        DIET_NAMES[j]
                    );
                }
            }
        }
    }

    /// The water runs from the surface's colour to the floor's, through the thermocline's layers,
    /// whatever odd thermocline the lab sets.
    #[test]
    fn the_water_runs_from_surface_to_floor() {
        assert_eq!(water(0.0, 15.0, 45.0), WORLD_TOP);
        assert_eq!(water(100.0, 15.0, 45.0), WORLD_BOTTOM);
        assert_eq!(water(15.0, 15.0, 45.0), WATER_WARM);
        assert_eq!(water(45.0, 15.0, 45.0), WATER_COLD);
        for (top, bottom) in [(0.0, 0.0), (100.0, 100.0), (60.0, 20.0), (0.0, 100.0)] {
            for depth in [0.0, 10.0, 50.0, 99.0, 100.0] {
                let _ = water(depth, top, bottom);
            }
        }
    }

    /// Text reads on every surface: the main text 12:1 and more, the muted and the diets' 5:1.
    #[test]
    fn text_reads_on_the_panels() {
        for ground in [PANEL, CARD, PANEL_TOP, CARD_HOVER] {
            assert!(contrast(TEXT, ground) >= 12.0);
            for c in [MUTED, ACCENT, GOOD, WARN, DANGER] {
                assert!(contrast(c, ground) >= 5.0, "{c:?} on {ground:?}: {:.1}", contrast(c, ground));
            }
            for d in DIET_COLORS {
                assert!(contrast(rgb(d), ground) >= 5.0, "{d:?} on {ground:?}");
            }
        }
        assert!(contrast(FAINT, BG) >= 5.0, "a footnote on the ground");
    }
}
