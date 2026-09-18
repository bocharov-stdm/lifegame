//! Мир на экране: фон полосами глубины, кружки (или карта плотности),
//! выделенное существо, миникарта. Рисует последний кадр потока симуляции и
//! никогда не ждёт нового.

use std::sync::Arc;
use std::time::Instant;

use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Vec2,
};
use eframe::egui_wgpu;
use life_core::Creature;

use crate::camera::{Camera, Viewport};
use crate::frame::{self, Frame, Instance, Raster, VEGETARIAN_COLOR, ViewRequest};
use crate::render::Circles;
use crate::sim::{Command, SimHandle};
use crate::theme::{ACCENT, BG, LINE, MUTED, rgb};

/// Полос глубины на фоне: у поверхности светлее, на глубине темнее.
const BANDS: usize = 32;
/// Сколько после последнего кадра ещё идут анимации (рост, угасание
/// призраков — `creatures.wgsl`): столько окно перерисовывается само.
const ANIMATION: f32 = 0.7;
/// Насколько можно промахнуться кликом по существу, точек экрана.
pub const PICK_RADIUS: f64 = 10.0;

/// Что случилось в мире по мыши за кадр.
pub enum Click {
    /// Клик по миру: точка мира и радиус промаха в единицах мира.
    World { x: f64, y: f64, radius: f64 },
}

#[derive(Default)]
pub struct WorldView {
    /// Последний кадр без кружков: кружки лежат отдельно, в `instances`.
    pub frame: Option<Frame>,
    instances: Arc<Vec<Instance>>,
    generation: u64,
    density: Option<(TextureHandle, (f64, f64, f64, f64))>,
    minimap: Option<TextureHandle>,
    pub camera: Option<Camera>,
    last_view: Option<ViewRequest>,
    /// Когда пришёл последний кадр и сглаженный промежуток между кадрами, с:
    /// по ним окно рисует движение между прошлым и новым кадром.
    arrived: Option<Instant>,
    interval: f64,
    /// Выделенное в прошлом кадре: кольцо едет вместе с кружком.
    prev_selected: Option<(Creature, f64, f64)>,
}

fn pos(x: f64, y: f64) -> Pos2 {
    Pos2::new(x as f32, y as f32)
}

fn image(r: &Raster) -> egui::ColorImage {
    egui::ColorImage::from_rgba_premultiplied([r.w, r.h], &r.rgba)
}

impl WorldView {
    /// Принять новый кадр. Приращения (история, хроника) забирает вызывающий.
    pub fn accept(&mut self, ctx: &egui::Context, sim: &SimHandle, mut f: Frame) {
        let fresh = Arc::new(std::mem::take(&mut f.instances));
        let old = std::mem::replace(&mut self.instances, fresh);
        // Прошлый буфер уже не нужен видеокарте — отдаём его обратно.
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
        self.prev_selected = self.frame.as_ref().and_then(|old| old.selected).map(|s| (s.creature, s.x, s.y));

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
        // новый мир — новая камера
        if self.frame.as_ref().is_some_and(|old| old.world_gen != f.world_gen) {
            self.camera = None;
            self.prev_selected = None;
        }
        self.frame = Some(f);
    }

    /// Нарисовать мир в прямоугольнике `rect`. `interactive` — можно ли
    /// двигать камеру и кликать (в меню мир только фон).
    pub fn show(
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

        // ── мышь: перетаскивание, колесо, клик ──────────────────────────────
        let mut click = None;
        if interactive {
            if response.dragged() {
                let d = response.drag_delta();
                cam.pan(d.x as f64, d.y as f64);
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

        // ── фон: мир полосами глубины ─────────────────────────────────────────
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

        // полоса слоя выбранного травоядного — где ему можно жить и есть
        if let Some(s) = f.selected
            && let Some((lo, hi)) = s.layer
        {
            let (_, y0) = cam.to_screen(0.0, lo);
            let (_, y1) = cam.to_screen(0.0, hi);
            let band = Rect::from_min_max(pos(left, y0), pos(right, y1.max(y0 + 1.0))).intersect(rect);
            painter.rect_filled(band, 0.0, rgb(VEGETARIAN_COLOR).gamma_multiply(0.07));
            let edge = Stroke::new(1.0, rgb(VEGETARIAN_COLOR).gamma_multiply(0.35));
            painter.hline(band.x_range(), y0 as f32, edge);
            painter.hline(band.x_range(), y1 as f32, edge);
        }

        // ── существа: кружки или карта плотности ─────────────────────────────
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
            if k < 1.0 || since < ANIMATION {
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
                },
            ));
        }
        painter.rect_stroke(world_rect, 0.0, Stroke::new(1.0, LINE), StrokeKind::Outside);

        // ── выделенное: кольцо вокруг тела и круг зрения ─────────────────────
        if let Some(s) = f.selected {
            let (x, y) = between(self.prev_selected, &s, k);
            let (sx, sy) = cam.to_screen(x, y);
            let vision = (s.vision * cam.zoom) as f32;
            if vision < 8000.0 {
                painter.circle_stroke(pos(sx, sy), vision, Stroke::new(1.0, ACCENT.gamma_multiply(0.45)));
            }
            let body = ((s.half * cam.zoom) as f32).max(2.0);
            painter.circle_stroke(pos(sx, sy), body + 5.0, Stroke::new(2.0, ACCENT));
        }

        // ── видимая область — потоку симуляции ────────────────────────────────
        let (x0, y0, x1, y1) = cam.visible_world();
        let ppp = ui.ctx().pixels_per_point();
        let req = ViewRequest {
            x0,
            y0,
            x1,
            y1,
            px_w: (rect.width() * ppp) as u32,
            px_h: (rect.height() * ppp) as u32,
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

    /// Миникарта в левом нижнем углу: где мы в мире. Клик или перетаскивание
    /// по ней переносит камеру. Когда виден весь мир, она не нужна.
    fn minimap(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let (Some(tex), Some(cam)) = (&self.minimap, &mut self.camera) else { return };
        if cam.is_fit() {
            return;
        }
        let aspect = (cam.world_h / cam.world_w) as f32;
        // не больше 30% ширины и 90 точек высоты: карта не должна заслонять мир
        let mut w = (rect.width() * 0.3).clamp(120.0, 320.0);
        let mut h = (w * aspect).max(14.0);
        if h > 90.0 {
            h = 90.0;
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
        // Совсем узкая рамка не видна — рисуем хотя бы в три точки.
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

    /// Доля пути от прошлого кадра к новому: окно рисует существ между ними.
    /// Без новых кадров (пауза) доходит до 1, и мир замирает.
    fn progress(&self) -> f32 {
        match self.arrived {
            Some(a) if self.interval > 0.0 => (a.elapsed().as_secs_f64() / self.interval).min(1.0) as f32,
            _ => 1.0,
        }
    }

    /// Слежение за выбранным: камера едет за ним, пока оно живо и выбрано —
    /// за той же точкой, где его рисует шейдер, иначе дёргался бы весь экран.
    pub fn follow_step(&mut self, dt: f64) {
        let k = self.progress();
        let (Some(cam), Some(f)) = (&mut self.camera, &self.frame) else { return };
        let Some(target) = cam.target else { return };
        let at = f
            .selected
            .filter(|s| creature_id(s.creature) == target)
            .map(|s| between(self.prev_selected, &s, k));
        cam.update(dt, at);
    }

    pub fn toggle_follow(&mut self) {
        let (Some(cam), Some(f)) = (&mut self.camera, &self.frame) else { return };
        match (cam.target, f.selected) {
            (Some(_), _) => cam.follow(None, None),
            (None, Some(s)) => cam.follow(Some(creature_id(s.creature)), Some((s.x, s.y))),
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

/// Где выделенное сейчас на экране: между прошлым кадром и новым.
fn between(prev: Option<(Creature, f64, f64)>, s: &frame::Selected, k: f32) -> (f64, f64) {
    match prev {
        Some((c, px, py)) if c == s.creature => (px + (s.x - px) * k as f64, py + (s.y - py) * k as f64),
        _ => (s.x, s.y),
    }
}

pub fn creature_id(c: Creature) -> u64 {
    match c {
        Creature::Vegetarian(id) | Creature::Predator(id) => id,
    }
}
