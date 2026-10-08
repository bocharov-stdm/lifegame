//! The world on the screen: a background in bands of depth, circles (or a density map), the
//! selected creature, the minimap. Draws the last frame of the simulation thread and never
//! waits for a new one.

use std::sync::Arc;
use std::time::Instant;

use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Vec2,
};
use eframe::egui_wgpu;

use crate::camera::{Camera, Viewport};
use crate::frame::{self, Area, CREATURE_COLOR, Frame, Instance, Raster, ViewRequest};
use crate::render::Circles;
use crate::sim::{Command, SimHandle};
use crate::theme::{ACCENT, BG, DANGER, LINE, MUTED, rgb};
use life_core::flora::Patch;

/// Bands of depth in the background: lighter near the surface, darker in the depths.
const BANDS: usize = 32;
/// How long after the last frame the animations still go on (growth, the ghosts' fading —
/// `creatures.wgsl`): for that long the window redraws itself.
const ANIMATION: f32 = 0.7;
/// By how much a click on a creature may miss, screen points.
pub const PICK_RADIUS: f64 = 10.0;
/// Less than this on a side, screen points — not a region, but a slip of the mouse.
const MIN_AREA: f64 = 4.0;

/// What happened in the world by the mouse in a frame.
pub enum Click {
    /// A click on the world: the world's point and the miss radius in world units.
    World { x: f64, y: f64, radius: f64 },
    /// A region has been dragged (the «Область» tool), in world coordinates.
    Area(Area),
}

#[derive(Default)]
pub struct WorldView {
    /// The last frame without circles: the circles lie apart, in `instances`.
    pub frame: Option<Frame>,
    instances: Arc<Vec<Instance>>,
    generation: u64,
    density: Option<(TextureHandle, (f64, f64, f64, f64))>,
    minimap: Option<TextureHandle>,
    pub camera: Option<Camera>,
    last_view: Option<ViewRequest>,
    /// When the last frame came and the smoothed interval between frames, s: by them the window
    /// draws the motion between the previous and the new frame.
    arrived: Option<Instant>,
    interval: f64,
    /// What was selected in the previous frame: the ring moves together with the circle.
    prev_selected: Option<(u64, f64, f64)>,
    /// The world's food patches, as the last frame that carried them had them.
    patches: Arc<[Patch]>,
    /// Dragging draws a region, not moves the camera (the «Область» tool).
    pub area_mode: bool,
    /// Where the drag of a region began, in world coordinates.
    drag_from: Option<(f64, f64)>,
    /// The last dragged region (no smaller than `MIN_AREA`).
    dragged_area: Option<Area>,
    /// The given region — drawn as a frame until it is cleared.
    pub area: Option<Area>,
    /// The time of building the world's draw commands on the CPU, ms.
    pub draw_ms: f64,
    /// Diets highlighted in the world, a bit per diet in `Diet` order (0 — none).
    pub highlight: u32,
}

fn pos(x: f64, y: f64) -> Pos2 {
    Pos2::new(x as f32, y as f32)
}

/// A patch smaller than this on screen, points, is not tinted: from afar the sprouts show it.
const MIN_PATCH: f64 = 6.0;

/// Tints the visible food patches: each an ellipse squashed against the plant zone's edges, a
/// soft rim and a slightly deeper heart.
fn paint_patches(painter: &egui::Painter, cam: &Camera, rect: Rect, patches: &[Patch]) {
    const SIDES: usize = 32;
    for p in patches {
        let (x, y) = cam.to_screen(p.x, p.y);
        let (l, r) = (p.left * cam.zoom, p.right * cam.zoom);
        let (u, d) = (p.up * cam.zoom, p.down * cam.zoom);
        if l.max(r).max(u).max(d) < MIN_PATCH
            || !Rect::from_min_max(pos(x - l, y - u), pos(x + r, y + d)).intersects(rect)
        {
            continue;
        }
        // slots per area: how dense the patch grows when full, relative to an even circle
        let area = std::f64::consts::FRAC_PI_4 * (p.left + p.right) * (p.up + p.down);
        let dense = (p.slots as f64 / area.max(1.0) * 1.2e3).clamp(0.3, 1.0);
        for (scale, alpha) in [(1.15, 7.0), (0.95, 9.0), (0.6, 8.0)] {
            let points = (0..SIDES)
                .map(|i| {
                    let (sin, cos) = (i as f64 / SIDES as f64 * std::f64::consts::TAU).sin_cos();
                    let dx = cos * if cos < 0.0 { l } else { r };
                    let dy = sin * if sin < 0.0 { u } else { d };
                    pos(x + dx * scale, y + dy * scale)
                })
                .collect();
            let a = (alpha * (0.5 + dense)) as u8;
            painter.add(egui::Shape::convex_polygon(
                points,
                Color32::from_rgba_unmultiplied(70, 150, 90, a),
                Stroke::NONE,
            ));
        }
    }
}

/// A region by two corners in any order, cut to the world's edges.
fn clamp_area(a: (f64, f64), b: (f64, f64), w: f64, h: f64) -> Area {
    let x = |v: f64| v.clamp(0.0, w);
    let y = |v: f64| v.clamp(0.0, h);
    (x(a.0.min(b.0)), y(a.1.min(b.1)), x(a.0.max(b.0)), y(a.1.max(b.1)))
}

fn image(r: &Raster) -> egui::ColorImage {
    egui::ColorImage::from_rgba_premultiplied([r.w, r.h], &r.rgba)
}

impl WorldView {
    pub fn cancel_area_drag(&mut self) {
        self.drag_from = None;
        self.dragged_area = None;
    }

    /// Accept a new frame. The increments (history, chronicle) are taken by the caller.
    pub fn accept(&mut self, ctx: &egui::Context, sim: &SimHandle, mut f: Frame) {
        let fresh = Arc::new(std::mem::take(&mut f.instances));
        let old = std::mem::replace(&mut self.instances, fresh);
        // The previous buffer is no longer needed by the graphics card — we hand it back.
        if let Ok(buf) = Arc::try_unwrap(old) {
            sim.recycle(buf);
        }
        self.generation += 1;

        let now = Instant::now();
        if let Some(last) = self.arrived {
            let gap = now.duration_since(last).as_secs_f64().clamp(1.0 / 240.0, 0.25);
            self.interval = if self.interval > 0.0 { self.interval * 0.8 + gap * 0.2 } else { gap };
        }
        self.arrived = Some(now);
        self.prev_selected = self.frame.as_ref().and_then(|old| old.selected).map(|s| (s.id, s.x, s.y));

        self.density = f.density.take().map(|r| {
            let tex = match self.density.take() {
                Some((mut tex, _)) => {
                    tex.set(image(&r), TextureOptions::LINEAR);
                    tex
                }
                None => ctx.load_texture("плотность", image(&r), TextureOptions::LINEAR),
            };
            (tex, r.rect)
        });
        if let Some(r) = f.minimap.take() {
            match &mut self.minimap {
                Some(tex) => tex.set(image(&r), TextureOptions::LINEAR),
                None => self.minimap = Some(ctx.load_texture("миникарта", image(&r), TextureOptions::LINEAR)),
            }
        }
        if let Some(patches) = f.patches.take() {
            self.patches = patches;
        }
        // a new world — a new camera
        if self.frame.as_ref().is_some_and(|old| old.world_gen != f.world_gen) {
            self.camera = None;
            self.prev_selected = None;
        }
        self.frame = Some(f);
    }

    /// Draw the world in the rectangle `rect`. `interactive` — whether the camera can be moved and
    /// clicks made (in the menu the world is only a background).
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        sim: &SimHandle,
        interactive: bool,
    ) -> Option<Click> {
        let start = Instant::now();
        let result = self.show_inner(ui, rect, sim, interactive);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        self.draw_ms = if self.draw_ms == 0.0 { ms } else { self.draw_ms * 0.85 + ms * 0.15 };
        result
    }

    fn show_inner(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        sim: &SimHandle,
        interactive: bool,
    ) -> Option<Click> {
        let sense = if interactive { Sense::click_and_drag() } else { Sense::hover() };
        let response = ui.allocate_rect(rect, sense);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, BG);

        let k = self.progress();
        let Some(f) = &self.frame else {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Создаём мир…",
                FontId::proportional(20.0),
                MUTED,
            );
            return None;
        };
        let view = Viewport {
            x: rect.min.x as f64,
            y: rect.min.y as f64,
            w: rect.width() as f64,
            h: rect.height() as f64,
        };
        let cam = self.camera.get_or_insert_with(|| Camera::new(f.world_w, f.world_h, view));
        cam.set_view(view);

        // ── mouse: dragging, wheel, click ───────────────────────────────────
        let mut click: Option<Click> = None;
        let mut drawing = None;
        if interactive {
            let to_world = |p: Pos2| cam.to_world(p.x as f64, p.y as f64);
            if self.area_mode {
                if response.drag_started() {
                    self.drag_from = response.interact_pointer_pos().map(to_world);
                    self.dragged_area = None;
                }
                let now = response.interact_pointer_pos().or(response.hover_pos()).map(to_world);
                if let (Some(a), Some(b)) = (self.drag_from, now) {
                    let area = clamp_area(a, b, f.world_w, f.world_h);
                    let big = (area.2 - area.0).min(area.3 - area.1) * cam.zoom >= MIN_AREA;
                    self.dragged_area = big.then_some(area);
                }
                // the pointer may already be gone in the release frame — we take
                // the last dragged region
                if response.drag_stopped() || ui.input(|i| i.pointer.any_released()) {
                    self.drag_from = None;
                    click = self.dragged_area.take().map(Click::Area);
                } else if self.drag_from.is_some() {
                    drawing = self.dragged_area;
                }
            } else {
                self.drag_from = None;
                self.dragged_area = None;
                if response.dragged() {
                    let d = response.drag_delta();
                    cam.pan(d.x as f64, d.y as f64);
                }
            }
            if response.hovered() {
                let (scroll, zoom, pointer) =
                    ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta(), i.pointer.hover_pos()));
                let factor = (scroll as f64 * 0.0025).exp() * zoom as f64;
                if let Some(p) = pointer
                    && factor != 1.0
                {
                    cam.zoom_at(p.x as f64, p.y as f64, factor);
                }
            }
            if response.clicked()
                && let Some(p) = response.interact_pointer_pos()
            {
                let (x, y) = cam.to_world(p.x as f64, p.y as f64);
                click = Some(Click::World { x, y, radius: PICK_RADIUS / cam.zoom });
            }
        }

        if !f.render_world {
            let (x0, y0, x1, y1) = cam.visible_world();
            let ppp = ui.ctx().pixels_per_point();
            let req = ViewRequest {
                x0,
                y0,
                x1,
                y1,
                px_w: (rect.width() * ppp) as u32,
                px_h: (rect.height() * ppp) as u32,
                highlight: self.highlight,
            };
            if self.last_view != Some(req) {
                self.last_view = Some(req);
                sim.send(Command::View(req));
            }
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Рендер мира выключен · симуляция продолжается",
                FontId::proportional(16.0),
                MUTED,
            );
            let screen = |a: Area| {
                let (x0, y0) = cam.to_screen(a.0, a.1);
                let (x1, y1) = cam.to_screen(a.2, a.3);
                Rect::from_min_max(pos(x0, y0), pos(x1, y1))
            };
            if let Some(a) = self.area {
                painter.rect_stroke(screen(a), 0.0, Stroke::new(1.5, ACCENT), StrokeKind::Outside);
            }
            if let Some(a) = drawing {
                painter.rect_filled(screen(a), 0.0, ACCENT.gamma_multiply(0.08));
                painter.rect_stroke(screen(a), 0.0, Stroke::new(1.0, ACCENT), StrokeKind::Outside);
            }
            return click;
        }

        // ── background: the world in bands of depth ───────────────────────────
        let (left, top) = cam.to_screen(0.0, 0.0);
        let (right, bottom) = cam.to_screen(f.world_w, f.world_h);
        let world_rect = Rect::from_min_max(pos(left, top), pos(right, bottom));
        let band_h = (bottom - top) / BANDS as f64;
        for i in 0..BANDS {
            let t = i as f64 / (BANDS - 1) as f64;
            let band = Rect::from_min_max(
                pos(left, top + band_h * i as f64),
                pos(right, top + band_h * (i + 1) as f64 + 0.5),
            );
            painter.rect_filled(
                band.intersect(rect),
                0.0,
                rgb(frame::lerp(frame::WORLD_TOP, frame::WORLD_BOTTOM, t)),
            );
        }

        // Food patches: a faint tint of the sea floor under the sprouts, so the islands read as
        // islands. Denser patches are a little darker green.
        let world_painter = painter.with_clip_rect(world_rect.intersect(rect));
        if !f.dots {
            paint_patches(&world_painter, cam, rect, &self.patches);
        }

        for corpse in &f.corpses {
            let (x, y) = cam.to_screen(corpse.x, corpse.py + (corpse.y - corpse.py) * k as f64);
            let center = pos(x, y);
            let body = if corpse.skeleton { 0.3 } else { 0.5 };
            let radius = (corpse.size * body * cam.zoom) as f32;
            if !Rect::from_center_size(center, Vec2::splat(radius * 2.0)).intersects(rect) {
                continue;
            }
            let alpha = (75.0 + 95.0 * corpse.fullness) as u8;
            let [r, g, b] = corpse.rgb();
            let fill = Color32::from_rgba_unmultiplied(r, g, b, alpha);
            if f.dots {
                painter.rect_filled(
                    Rect::from_center_size(center, Vec2::splat(2.0 / ui.ctx().pixels_per_point())),
                    0.0,
                    fill,
                );
                continue;
            }
            world_painter.circle_filled(center, radius, fill);
            let rim = |c: u8| c.saturating_add(50);
            world_painter.circle_stroke(
                center,
                radius,
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(rim(r), rim(g), rim(b), alpha)),
            );
        }

        // the selected creature's layer band — where it may live and eat
        if let Some(s) = f.selected {
            let (lo, hi) = s.layer;
            let (_, y0) = cam.to_screen(0.0, lo);
            let (_, y1) = cam.to_screen(0.0, hi);
            let band = Rect::from_min_max(pos(left, y0), pos(right, y1.max(y0 + 1.0))).intersect(rect);
            painter.rect_filled(band, 0.0, rgb(CREATURE_COLOR).gamma_multiply(0.07));
            let edge = Stroke::new(1.0, rgb(CREATURE_COLOR).gamma_multiply(0.35));
            painter.hline(band.x_range(), y0 as f32, edge);
            painter.hline(band.x_range(), y1 as f32, edge);
        }

        // ── creatures: circles or a density map ──────────────────────────────
        if let Some((tex, r)) = &self.density {
            let (x0, y0) = cam.to_screen(r.0, r.1);
            let (x1, y1) = cam.to_screen(r.2, r.3);
            painter.image(
                tex.id(),
                Rect::from_min_max(pos(x0, y0), pos(x1, y1)),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        } else if !self.instances.is_empty() {
            let (ox, oy) = cam.to_screen(f.origin.0, f.origin.1);
            let since = f.built.map_or(ANIMATION, |b| b.elapsed().as_secs_f32());
            if !f.dots && (k < 1.0 || since < ANIMATION) {
                ui.ctx().request_repaint();
            }
            painter.add(egui_wgpu::Callback::new_paint_callback(
                rect,
                Circles {
                    instances: self.instances.clone(),
                    generation: self.generation,
                    origin: [(ox - view.x) as f32, (oy - view.y) as f32],
                    view: [rect.width(), rect.height()],
                    zoom: cam.zoom as f32,
                    pixels_per_point: ui.ctx().pixels_per_point(),
                    k,
                    since,
                    time: (ui.ctx().input(|i| i.time) % 1000.0) as f32,
                    highlight: self.highlight,
                },
            ));
        }
        let shot_painter = painter.with_clip_rect(world_rect.intersect(rect));
        for shot in &f.shots {
            let age = shot.age + f.built.map_or(0.0, |built| built.elapsed().as_secs_f32());
            if age >= 0.25 {
                continue;
            }
            let (x0, y0) = cam.to_screen(shot.from.0, shot.from.1);
            let (x1, y1) = cam.to_screen(shot.to.0, shot.to.1);
            let alpha = ((1.0 - age / 0.25) * 220.0) as u8;
            shot_painter.line_segment(
                [pos(x0, y0), pos(x1, y1)],
                Stroke::new(1.6, Color32::from_rgba_unmultiplied(255, 216, 122, alpha)),
            );
            shot_painter.circle_filled(
                pos(x1, y1),
                2.0,
                Color32::from_rgba_unmultiplied(255, 216, 122, alpha),
            );
            ui.ctx().request_repaint();
        }
        painter.rect_stroke(world_rect, 0.0, Stroke::new(1.0, LINE), StrokeKind::Outside);

        // ── region: a given one as a frame, one being dragged as a filled frame ───
        let screen = |a: Area| {
            let (x0, y0) = cam.to_screen(a.0, a.1);
            let (x1, y1) = cam.to_screen(a.2, a.3);
            Rect::from_min_max(pos(x0, y0), pos(x1, y1))
        };
        if let Some(a) = self.area {
            painter.rect_stroke(screen(a), 0.0, Stroke::new(1.5, ACCENT), StrokeKind::Outside);
        }
        if let Some(a) = drawing {
            painter.rect_filled(screen(a), 0.0, ACCENT.gamma_multiply(0.08));
            painter.rect_stroke(screen(a), 0.0, Stroke::new(1.0, ACCENT), StrokeKind::Outside);
        }

        // ── selected: a ring round the body and the circle of sight ──────────
        if let Some(s) = f.selected {
            let (x, y) = between(self.prev_selected, &s, k);
            let (sx, sy) = cam.to_screen(x, y);
            let vision = (s.vision * cam.zoom) as f32;
            if vision < 8000.0 {
                painter.circle_stroke(pos(sx, sy), vision, Stroke::new(1.0, ACCENT.gamma_multiply(0.45)));
            }
            let body = ((s.half * cam.zoom) as f32).max(2.0);
            painter.circle_stroke(pos(sx, sy), body, Stroke::new(1.0, DANGER.gamma_multiply(0.8)));
            painter.circle_stroke(pos(sx, sy), body + 5.0, Stroke::new(2.0, ACCENT));
        }

        // ── the visible area — to the simulation thread ───────────────────────
        let (x0, y0, x1, y1) = cam.visible_world();
        let ppp = ui.ctx().pixels_per_point();
        let req = ViewRequest {
            x0,
            y0,
            x1,
            y1,
            px_w: (rect.width() * ppp) as u32,
            px_h: (rect.height() * ppp) as u32,
            highlight: self.highlight,
        };
        if self.last_view != Some(req) {
            self.last_view = Some(req);
            sim.send(Command::View(req));
        }

        if interactive {
            self.minimap(ui, rect);
        }
        click
    }

    /// The minimap in the bottom left corner: where we are in the world. A click or a drag on it
    /// moves the camera. When the whole world is visible, it is not needed.
    fn minimap(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let (Some(tex), Some(cam)) = (&self.minimap, &mut self.camera) else { return };
        if cam.is_fit() {
            return;
        }
        let aspect = (cam.world_h / cam.world_w) as f32;
        // no more than 30% of the width and 90 points of height (130 for a world up to 4:1 wide:
        // otherwise it shrinks into a stamp): the map must not cover the world
        let max_h = if aspect > 0.25 { 130.0 } else { 90.0 };
        let mut w = (rect.width() * 0.3).clamp(120.0, 320.0);
        let mut h = (w * aspect).max(14.0);
        if h > max_h {
            h = max_h;
            w = h / aspect;
        }
        let map = Rect::from_min_size(Pos2::new(rect.min.x + 10.0, rect.max.y - h - 10.0), Vec2::new(w, h));
        let resp = ui.interact(map, ui.id().with("миникарта"), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(map.expand(3.0), 3.0, Color32::from_rgba_unmultiplied(12, 14, 18, 220));
        painter.image(tex.id(), map, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        let to_map = |x: f64, y: f64| {
            pos(
                map.min.x as f64 + x / cam.world_w * map.width() as f64,
                map.min.y as f64 + y / cam.world_h * map.height() as f64,
            )
        };
        let (x0, y0, x1, y1) = cam.visible_world();
        let mut seen = Rect::from_min_max(
            to_map(x0.max(0.0), y0.max(0.0)),
            to_map(x1.min(cam.world_w), y1.min(cam.world_h)),
        );
        // A very narrow frame is not visible — we draw at least three points.
        if seen.width() < 3.0 {
            seen = Rect::from_center_size(seen.center(), Vec2::new(3.0, seen.height()));
        }
        painter.rect_stroke(seen, 0.0, Stroke::new(1.5, ACCENT), StrokeKind::Outside);
        if (resp.clicked() || resp.dragged())
            && let Some(p) = resp.interact_pointer_pos()
        {
            let x = ((p.x - map.min.x) / map.width()) as f64 * cam.world_w;
            let y = ((p.y - map.min.y) / map.height()) as f64 * cam.world_h;
            cam.center_on(x, y);
        }
    }

    /// The share of the way from the previous frame to the new one: the window draws creatures
    /// between them. Without new frames (pause) it reaches 1, and the world freezes. A click carries
    /// it, to pick a body where it was drawn (`Command::Pick`).
    pub fn progress(&self) -> f32 {
        match self.arrived {
            Some(a) if self.interval > 0.0 => (a.elapsed().as_secs_f64() / self.interval).min(1.0) as f32,
            _ => 1.0,
        }
    }

    /// Following the selected one: the camera goes after it while it lives and is selected — after
    /// the same point where the shader draws it, otherwise the whole screen would jerk.
    pub fn follow_step(&mut self, dt: f64) {
        let k = self.progress();
        let (Some(cam), Some(f)) = (&mut self.camera, &self.frame) else { return };
        let Some(target) = cam.target else { return };
        let at = f.selected.filter(|s| s.id == target).map(|s| between(self.prev_selected, &s, k));
        cam.update(dt, at);
    }

    pub fn toggle_follow(&mut self) {
        let (Some(cam), Some(f)) = (&mut self.camera, &self.frame) else { return };
        match (cam.target, f.selected) {
            (Some(_), _) => cam.follow(None, None),
            (None, Some(s)) => cam.follow(Some(s.id), Some((s.x, s.y))),
            (None, None) => {}
        }
    }

    #[cfg(test)]
    pub fn instances(&self) -> &[Instance] {
        &self.instances
    }

    pub fn following(&self) -> bool {
        self.camera.as_ref().is_some_and(|c| c.target.is_some())
    }
}

/// Where the selected creature is on the screen now: between the previous frame and the new one.
fn between(prev: Option<(u64, f64, f64)>, s: &frame::Selected, k: f32) -> (f64, f64) {
    match prev {
        Some((id, px, py)) if id == s.id => (px + (s.x - px) * k as f64, py + (s.y - py) * k as f64),
        _ => (s.x, s.y),
    }
}
