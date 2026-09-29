//! The camera: which part of the world is visible in the viewport and at what scale. A port of
//! `app/camera.py` (tag python-final).
//!
//! The world at a big scale is very elongated: at ×1000 it is 1500 times wider than tall. So
//! the world's coordinates are f64 (a width up to 6·10⁷), and the screen's are f32. The
//! viewport is a rectangle in screen points (egui).

/// 1 world unit = 4 screen points: there is no point in looking at a creature any closer.
pub const MAX_ZOOM: f64 = 4.0;
/// The smoothness of following: the bigger, the tighter the camera holds to its target.
pub const FOLLOW_RATE: f64 = 8.0;

/// The viewport's rectangle on the screen: the top left corner and the size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Debug)]
pub struct Camera {
    pub world_w: f64,
    pub world_h: f64,
    pub view: Viewport,
    pub zoom: f64,
    pub cx: f64,
    pub cy: f64,
    /// Whom we follow (a creature's id) and where the target was in the previous frame.
    pub target: Option<u64>,
    last: Option<(f64, f64)>,
}

impl Camera {
    /// A new camera: the whole world in the centre. A very elongated world (a big scale) — as a
    /// strip the full height, in which the creatures are still visible.
    pub fn new(world_w: f64, world_h: f64, view: Viewport) -> Self {
        let mut cam = Camera {
            world_w,
            world_h,
            view,
            zoom: 1.0,
            cx: world_w / 2.0,
            cy: world_h / 2.0,
            target: None,
            last: None,
        };
        // An elongated world fitted whole is a thread of dots; then by height.
        let by_height = view.h / world_h;
        let zoom = if world_w * by_height > 3.0 * view.w { by_height } else { cam.min_zoom() };
        cam.zoom = zoom.clamp(cam.min_zoom(), cam.max_zoom());
        cam.clamp();
        cam
    }

    /// The whole world in the viewport — no point zooming out further.
    pub fn min_zoom(&self) -> f64 {
        (self.view.w / self.world_w).min(self.view.h / self.world_h).max(1e-9)
    }

    pub fn max_zoom(&self) -> f64 {
        MAX_ZOOM.max(self.min_zoom())
    }

    pub fn is_fit(&self) -> bool {
        self.zoom <= self.min_zoom() * 1.0001
    }

    /// Show the whole world.
    pub fn fit(&mut self) {
        self.zoom = self.min_zoom();
        self.cx = self.world_w / 2.0;
        self.cy = self.world_h / 2.0;
        self.follow(None, None);
    }

    /// A new viewport size. The centre stays in place, a fitted world stays fitted.
    pub fn set_view(&mut self, view: Viewport) {
        if view == self.view {
            return;
        }
        let was_fit = self.is_fit();
        self.view = view;
        if was_fit {
            self.zoom = self.min_zoom();
        }
        self.zoom = self.zoom.clamp(self.min_zoom(), self.max_zoom());
        self.clamp();
    }

    /// Change the scale so that the world point under (sx, sy) stays under the cursor.
    pub fn zoom_at(&mut self, sx: f64, sy: f64, factor: f64) {
        let (wx, wy) = self.to_world(sx, sy);
        self.zoom = (self.zoom * factor).clamp(self.min_zoom(), self.max_zoom());
        self.cx = wx - (sx - self.view.x - self.view.w / 2.0) / self.zoom;
        self.cy = wy - (sy - self.view.y - self.view.h / 2.0) / self.zoom;
        if let (Some(_), Some((x, y))) = (self.target, self.last) {
            // we follow, so the centre is on the target
            self.cx = x;
            self.cy = y;
        }
        self.clamp();
    }

    /// A shift by (dx, dy) screen points: the world moves after the mouse.
    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.cx -= dx / self.zoom;
        self.cy -= dy / self.zoom;
        self.follow(None, None);
        self.clamp();
    }

    /// Point the camera's centre at a world point (a click on the minimap).
    pub fn center_on(&mut self, x: f64, y: f64) {
        self.cx = x;
        self.cy = y;
        self.follow(None, None);
        self.clamp();
    }

    /// Follow the creature `id`, which is now at `at`; None — stop.
    pub fn follow(&mut self, id: Option<u64>, at: Option<(f64, f64)>) {
        self.target = id;
        self.last = if id.is_some() { at } else { None };
    }

    /// Following: the camera moves with the target and smoothly brings it to the centre.
    /// `at` — where the target is now; None — the target is gone (died), following is dropped.
    ///
    /// Smooth catching up alone is not enough: at high speed a creature goes farther in a frame
    /// than the camera can catch up, and disappears from the screen. So first the camera shifts by
    /// as much as the target shifted, and the smoothness is left only for the rest of the way to
    /// the centre.
    pub fn update(&mut self, dt: f64, at: Option<(f64, f64)>) {
        if self.target.is_none() {
            return;
        }
        let Some((x, y)) = at else {
            self.follow(None, None);
            return;
        };
        let (lx, ly) = self.last.unwrap_or((x, y));
        self.cx += x - lx;
        self.cy += y - ly;
        self.last = Some((x, y));
        let k = 1.0 - (-FOLLOW_RATE * dt).exp();
        self.cx += (x - self.cx) * k;
        self.cy += (y - self.cy) * k;
        self.clamp();
    }

    /// The world fills the viewport as far as it can; if it is smaller, it stands in the centre.
    fn clamp(&mut self) {
        let half_w = self.view.w / 2.0 / self.zoom;
        let half_h = self.view.h / 2.0 / self.zoom;
        self.cx = if 2.0 * half_w >= self.world_w {
            self.world_w / 2.0
        } else {
            self.cx.clamp(half_w, self.world_w - half_w)
        };
        self.cy = if 2.0 * half_h >= self.world_h {
            self.world_h / 2.0
        } else {
            self.cy.clamp(half_h, self.world_h - half_h)
        };
    }

    pub fn to_screen(&self, wx: f64, wy: f64) -> (f64, f64) {
        (
            self.view.x + self.view.w / 2.0 + (wx - self.cx) * self.zoom,
            self.view.y + self.view.h / 2.0 + (wy - self.cy) * self.zoom,
        )
    }

    pub fn to_world(&self, sx: f64, sy: f64) -> (f64, f64) {
        (
            self.cx + (sx - self.view.x - self.view.w / 2.0) / self.zoom,
            self.cy + (sy - self.view.y - self.view.h / 2.0) / self.zoom,
        )
    }

    /// The visible rectangle of the world (x0, y0, x1, y1).
    pub fn visible_world(&self) -> (f64, f64, f64, f64) {
        let (x0, y0) = self.to_world(self.view.x, self.view.y);
        let (x1, y1) = self.to_world(self.view.x + self.view.w, self.view.y + self.view.h);
        (x0, y0, x1, y1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: Viewport = Viewport { x: 10.0, y: 20.0, w: 800.0, h: 500.0 };

    #[test]
    fn зум_к_курсору_оставляет_точку_под_курсором() {
        let mut cam = Camera::new(6000.0, 4000.0, VIEW);
        let (sx, sy) = (300.0, 200.0);
        let before = cam.to_world(sx, sy);
        cam.zoom_at(sx, sy, 3.0);
        let after = cam.to_world(sx, sy);
        assert!((before.0 - after.0).abs() < 1e-6 && (before.1 - after.1).abs() < 1e-6);
    }

    #[test]
    fn мир_не_уезжает_из_вьюпорта() {
        let mut cam = Camera::new(6000.0, 4000.0, VIEW);
        cam.zoom_at(400.0, 250.0, 10.0);
        cam.pan(1e7, 1e7);
        let (x0, y0, _, _) = cam.visible_world();
        assert!(x0.abs() < 1e-6 && y0.abs() < 1e-6, "левый верхний угол мира в углу вьюпорта");
        cam.fit();
        assert!(cam.is_fit());
        let (x0, _, x1, _) = cam.visible_world();
        assert!(x0 <= 0.0 && x1 >= 6000.0, "весь мир виден");
    }

    #[test]
    fn вытянутый_мир_стартует_во_всю_высоту() {
        // ×1000: a world of 6 000 000 x 4000
        let cam = Camera::new(6e6, 4000.0, VIEW);
        let (_, y0, _, y1) = cam.visible_world();
        assert!((y0 - 0.0).abs() < 1e-6 && (y1 - 4000.0).abs() < 1e-6);
        assert!((cam.cx - 3e6).abs() < 1e-6);
    }

    #[test]
    fn слежение_не_теряет_быструю_цель() {
        let mut cam = Camera::new(6000.0, 4000.0, VIEW);
        cam.zoom_at(400.0, 250.0, 8.0);
        cam.follow(Some(7), Some((3000.0, 2000.0)));
        // the target jumps 300 units a frame — on the screen that is far past the edge
        for i in 1..=10 {
            let at = (3000.0 + 300.0 * i as f64, 2000.0);
            cam.update(1.0 / 60.0, Some(at));
            let (sx, _) = cam.to_screen(at.0, at.1);
            assert!((VIEW.x..=VIEW.x + VIEW.w).contains(&sx), "цель на экране");
        }
        cam.update(1.0 / 60.0, None);
        assert_eq!(cam.target, None, "умершая цель снимает слежение");
    }

    #[test]
    fn изменение_окна_сохраняет_вписанный_мир() {
        let mut cam = Camera::new(6000.0, 4000.0, VIEW);
        cam.fit();
        cam.set_view(Viewport { x: 0.0, y: 0.0, w: 1600.0, h: 900.0 });
        assert!(cam.is_fit());
    }
}
