//! The selected creature's behaviour programs as flowcharts, in their own window «Поведение»: two
//! tabs, the juvenile track (while it grows) and the adult one, and the modes on in the header. The
//! settings come first, as the engine runs them: of each kind the first whose conditions hold
//! applies, wherever it stands in the program (a pill-shaped box, its down arrow «дальше»). Then the
//! deciding blocks in order — a condition of up to three tests, «да» to its action with the
//! action's parameters, «нет» down to the next — ending in «ничего не подошло: стоит». Blocks keep
//! their numbers in the program. On the track it lives by now the path of the current tick is lit:
//! the settings that applied, the blocks whose condition held but whose action could not be done
//! («не вышло»), and the block that decided. Deciding blocks after one that always fires are
//! faded: their turn never comes. Hovering a box tells what it checks or does.

use eframe::egui::text::LayoutJob;
use eframe::egui::{self, Align2, Color32, FontId, Galley, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use life_core::creature::program::{MODES, ticks_word};
use life_core::creature::strategy::VARIANTS as STRATEGIES;
use life_core::creature::{ADULT, Action, Block, JUVENILE, Program};
use life_core::genome::creature::Gene;
use std::sync::Arc;

use crate::frame::Selected;
use crate::theme::{ACCENT, CARD, DANGER, DIET_COLORS, LINE, MUTED, TEXT, rgb};

/// Left margin for the block numbers, the condition and action boxes and the gap between them.
const NUMBER_W: f32 = 26.0;
const COND_W: f32 = 236.0;
const GAP: f32 = 58.0;
const ACTION_W: f32 = 190.0;
const WIDTH: f32 = NUMBER_W + COND_W + GAP + ACTION_W + 6.0;
/// A condition box's least height (a row is as tall as its taller box) and the room below a row
/// for the «нет» arrow.
const BOX_H: f32 = 46.0;
const ARROW_H: f32 = 24.0;
const START_H: f32 = 22.0;
const END_H: f32 = 34.0;
/// The band above a section (settings, decisions) with its caption.
const CAPTION_H: f32 = 20.0;
/// The colour of settings' boxes.
const SETTING: Color32 = Color32::from_rgb(96, 190, 170);

/// The window, while `open` and a creature is selected. The tab shown is remembered per creature;
/// a newly selected one opens on the track it lives by.
pub(crate) fn behaviour_window(ctx: &egui::Context, open: &mut bool, s: &Selected) {
    egui::Window::new(format!("Поведение № {}", s.id))
        .id(egui::Id::new("поведение"))
        .open(open)
        .resizable(false)
        .default_pos(ctx.content_rect().left_top() + Vec2::new(24.0, 60.0))
        .show(ctx, |ui| {
            ui.set_width(WIDTH);
            let memory = egui::Id::new("поведение-дорожка");
            let (mut who, mut tab) = ctx.data(|d| d.get_temp::<(u64, usize)>(memory)).unwrap_or((0, s.stage));
            if who != s.id {
                (who, tab) = (s.id, s.stage);
            }
            let template =
                STRATEGIES.get(s.genome[Gene::Strategy as usize] as usize).map_or("?", |v| v.label);
            ui.horizontal(|ui| {
                ui.label(format!("Шаблон: {template} · дорожка:"));
                for (stage, name) in [(JUVENILE, "детская"), (ADULT, "взрослая")] {
                    let text = if stage == s.stage { format!("{name} ●") } else { name.to_string() };
                    let hint = match stage {
                        JUVENILE => "Программа, по которой живёт, пока растёт до своего размера.",
                        _ => "Программа, по которой живёт, когда вырос.",
                    };
                    if ui.selectable_label(tab == stage, text).on_hover_text(hint).clicked() {
                        tab = stage;
                    }
                }
            });
            ctx.data_mut(|d| d.insert_temp(memory, (who, tab)));
            let p = &s.programs[tab];
            let n = p.blocks().len();
            ui.label(format!(
                "Мутаций от шаблона: {} · блоков: {n}{}",
                p.changes,
                if (0..n).any(|i| !p.live(i)) {
                    format!(", работают {}", (0..n).filter(|&i| p.live(i)).count())
                } else {
                    String::new()
                }
            ))
            .on_hover_text(
                "Считаются мутации, которые что-то изменили: число, условие, действие, порядок, копия, \
                 удаление, новый блок. Числа ещё и дрейфуют понемногу у каждого мутирующего ребёнка — \
                 это мутацией не считается.",
            );
            let on: Vec<String> = (0..MODES)
                .filter(|&k| s.modes[k] > 0)
                .map(|k| {
                    let left = s.modes[k].min(u64::from(u16::MAX)) as u16;
                    format!("{} (ещё {left} {})", k + 1, ticks_word(left))
                })
                .collect();
            let modes = if on.is_empty() { "все выключены".to_string() } else { on.join(", ") };
            ui.label(format!("Режимы: {modes}")).on_hover_text(
                "Режим — память программы: установка «режим» включает его на время, условие «режим» \
                 проверяет. Режимы общие для обеих дорожек.",
            );
            let live = tab == s.stage;
            let note = if live {
                "Каждый тик: сначала установки (овалы) — из каждого рода первая, чьё условие выполнено, \
                 где бы она ни стояла; потом решает первый блок, чьё условие выполнено и чьё действие \
                 возможно. Жёлтым — путь этого тика."
            } else {
                "Каждый тик: сначала установки (овалы) — из каждого рода первая, чьё условие выполнено, \
                 где бы она ни стояла; потом решает первый блок, чьё условие выполнено и чьё действие \
                 возможно. Сейчас живёт по другой дорожке (●)."
            };
            ui.add(egui::Label::new(egui::RichText::new(note).color(MUTED)).wrap());
            ui.add_space(4.0);
            // as tall as the screen allows, scrolling a long program
            let height = (ctx.content_rect().height() - 250.0).clamp(140.0, 1200.0);
            let path = live.then_some(Path { fired: s.fired, applied: s.applied, tried: s.tried });
            egui::ScrollArea::vertical()
                .id_salt(tab)
                .max_height(height)
                .min_scrolled_height(height)
                .show(ui, |ui| flowchart(ui, p, path));
        });
}

/// This tick's way through the program it lives by (bit i: block i).
#[derive(Clone, Copy)]
struct Path {
    fired: Option<u8>,
    applied: u32,
    tried: u32,
}

impl Path {
    fn has(bits: u32, i: usize) -> bool {
        i < 32 && bits & (1 << i) != 0
    }
}

/// The colour of an action's box: fights red, food by the diet that lives on it, standing still
/// violet, settings teal, moves blue.
fn action_color(a: Action) -> Color32 {
    match a {
        Action::FightBack | Action::Flee => DANGER,
        Action::Hunt => rgb(DIET_COLORS[3]),
        Action::EatCorpse => rgb(DIET_COLORS[2]),
        Action::EatPlant => rgb(DIET_COLORS[0]),
        Action::Ambush | Action::Rest | Action::Torpor => Color32::from_rgb(176, 160, 214),
        a if a.is_setting() => SETTING,
        _ => Color32::from_rgb(110, 165, 230),
    }
}

/// Text wrapped to `width`.
fn layout(ui: &egui::Ui, text: String, size: f32, color: Color32, width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple(text, FontId::proportional(size), color, width);
    job.halign = egui::Align::Center;
    ui.painter().layout_job(job)
}

/// An action box's text: a caption for a setting, the action, its parameters a line each.
struct ActionText {
    caption: Option<Arc<Galley>>,
    label: Arc<Galley>,
    params: Option<Arc<Galley>>,
}

impl ActionText {
    fn of(ui: &egui::Ui, b: &Block, color: Color32) -> Self {
        let width = ACTION_W - 14.0;
        let caption = b.action.is_setting().then(|| layout(ui, "установка".into(), 10.5, MUTED, width));
        let label = layout(ui, b.action.label().into(), 14.0, color, width);
        let lines: Vec<String> = b.action.params().iter().zip(b.args).map(|(p, raw)| p.show(raw)).collect();
        let params = (!lines.is_empty()).then(|| layout(ui, lines.join("\n"), 11.5, MUTED, width));
        ActionText { caption, label, params }
    }

    fn height(&self) -> f32 {
        let caption = self.caption.as_ref().map_or(0.0, |g| g.size().y + 1.0);
        let params = self.params.as_ref().map_or(0.0, |g| g.size().y + 2.0);
        caption + self.label.size().y + params
    }

    /// Painted centred in `rect`.
    fn paint(self, painter: &egui::Painter, rect: Rect) {
        let mut y = rect.center().y - self.height() / 2.0;
        let x = rect.center().x;
        for (galley, gap) in [(self.caption, 1.0), (Some(self.label), 2.0), (self.params, 0.0)] {
            if let Some(g) = galley {
                let h = g.size().y;
                // a centred job lays its lines around x = 0
                painter.galley(Pos2::new(x, y), g, TEXT);
                y += h + gap;
            }
        }
    }
}

fn flowchart(ui: &mut egui::Ui, p: &Program, path: Option<Path>) {
    let blocks = p.blocks();
    let reachable = p.reachable();
    let ending = p.ending();
    // the settings first, as the engine applies them, then the deciding blocks in order
    let settings: Vec<usize> = (0..blocks.len()).filter(|&i| blocks[i].action.is_setting()).collect();
    let deciders: Vec<usize> = (0..blocks.len()).filter(|&i| !blocks[i].action.is_setting()).collect();
    let order: Vec<usize> = settings.iter().chain(&deciders).copied().collect();
    let texts: Vec<ActionText> =
        order.iter().map(|&i| ActionText::of(ui, &blocks[i], if p.live(i) { TEXT } else { MUTED })).collect();
    // a condition of three tests wraps to more lines: its box grows, and the row with it
    let conditions: Vec<Arc<Galley>> = order
        .iter()
        .map(|&i| {
            let b = &blocks[i];
            let dead = (!b.action.is_setting() && i >= reachable) || b.off();
            let color = if dead { MUTED } else { TEXT };
            layout(ui, format!("{}?", b.condition_label()), 13.0, color, COND_W - 30.0)
        })
        .collect();
    let cond_h: Vec<f32> = conditions.iter().map(|g| BOX_H.max(g.size().y + 14.0)).collect();
    let box_h: Vec<f32> =
        texts.iter().zip(&cond_h).map(|(t, &c)| c.max(BOX_H.max(t.height() + 12.0))).collect();
    // the sections' captions: above the settings, if any, and above the decisions
    let captions = usize::from(!settings.is_empty()) + 1;
    let rows: f32 = box_h.iter().map(|h| h + ARROW_H).sum();
    let height = START_H + 14.0 + captions as f32 * CAPTION_H + rows + END_H + 4.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(WIDTH, height), Sense::hover());
    let painter = ui.painter_at(rect);
    let lit = Stroke::new(2.5, ACCENT);
    let tried_stroke = Stroke::new(1.5, ACCENT);
    let plain = Stroke::new(1.3, MUTED);
    let faded = Stroke::new(1.0, LINE);
    let cond_x = rect.left() + NUMBER_W;
    let mid = cond_x + COND_W / 2.0;
    let action_x = cond_x + COND_W + GAP;
    let fired = path.and_then(|p| p.fired);
    // how far down the deciding blocks this tick went: the one that decided, or all of them
    let depth = path.map(|p| p.fired.map_or(usize::MAX, usize::from));

    // where each row and the end stand, with the captions' bands
    let mut tops = Vec::with_capacity(order.len());
    let mut caption_at = Vec::new();
    let mut y = rect.top() + START_H + 14.0;
    for (k, h) in box_h.iter().enumerate() {
        if k == 0 || k == settings.len() {
            caption_at.push((y, k < settings.len()));
            y += CAPTION_H;
        }
        tops.push(y);
        y += h + ARROW_H;
    }
    if deciders.is_empty() {
        caption_at.push((y, false));
        y += CAPTION_H;
    }
    let end_top = y;

    let start = Rect::from_center_size(Pos2::new(mid, rect.top() + START_H / 2.0), Vec2::new(90.0, START_H));
    let start_stroke = if path.is_some() { lit } else { plain };
    painter.rect(start, 11.0, CARD, start_stroke, egui::StrokeKind::Inside);
    painter.text(start.center(), Align2::CENTER_CENTER, "тик", FontId::proportional(13.0), TEXT);
    let first = tops.first().copied().unwrap_or(end_top);
    painter.arrow(Pos2::new(mid, start.bottom()), Vec2::new(0.0, first - start.bottom() - 2.0), start_stroke);
    for (y, of_settings) in caption_at {
        let caption = if of_settings {
            "сначала — установки"
        } else {
            "потом — первый, кто смог"
        };
        painter.text(
            Pos2::new(action_x, y + CAPTION_H / 2.0),
            Align2::LEFT_CENTER,
            caption,
            FontId::proportional(11.5),
            MUTED,
        );
    }

    for (k, ((&i, text), galley)) in order.iter().zip(texts).zip(conditions).enumerate() {
        let b = &blocks[i];
        let h = box_h[k];
        let ch = cond_h[k];
        let top = tops[k];
        let next = tops.get(k + 1).copied().unwrap_or(end_top);
        let setting = b.action.is_setting();
        // a deciding block after one that always fires is never reached; a «never» test switches
        // a block off
        let unreached = !setting && i >= reachable;
        let off = b.off();
        let dead = unreached || off;
        let decided = fired == Some(i as u8);
        let applied = path.is_some_and(|p| setting && Path::has(p.applied, i));
        let tried = !decided && path.is_some_and(|p| !setting && Path::has(p.tried, i));
        // a setting is skipped, its condition not looked at, when one of its kind above it applied
        let skipped = path.is_some_and(|p| {
            setting
                && (0..i).any(|j| {
                    blocks[j].action.is_setting()
                        && blocks[j].setting_kind() == b.setting_kind()
                        && Path::has(p.applied, j)
                })
        });
        // the settings are looked at but the skipped; the deciding blocks down to the one that
        // decided
        let on_path = depth.is_some_and(|d| if setting { !skipped } else { i <= d });
        let cond = Rect::from_min_size(Pos2::new(cond_x, top), Vec2::new(COND_W, ch));
        let action = Rect::from_min_size(Pos2::new(action_x, top), Vec2::new(ACTION_W, h));

        // the number, and a marker at the block that decided
        let number_color = if decided || applied { ACCENT } else { MUTED };
        painter.text(
            Pos2::new(rect.left() + 9.0, cond.center().y),
            Align2::CENTER_CENTER,
            format!("{}", i + 1),
            FontId::proportional(13.0),
            number_color,
        );
        if decided {
            let (x, y) = (cond.left() - 5.0, cond.center().y);
            painter.add(Shape::convex_polygon(
                vec![Pos2::new(x - 7.0, y - 6.0), Pos2::new(x, y), Pos2::new(x - 7.0, y + 6.0)],
                ACCENT,
                Stroke::NONE,
            ));
        }

        // the condition: a hexagon, the decision of a flowchart
        let (l, r, t, bottom, cy) = (cond.left(), cond.right(), cond.top(), cond.bottom(), cond.center().y);
        let hexagon = vec![
            Pos2::new(l + 12.0, t),
            Pos2::new(r - 12.0, t),
            Pos2::new(r, cy),
            Pos2::new(r - 12.0, bottom),
            Pos2::new(l + 12.0, bottom),
            Pos2::new(l, cy),
        ];
        let cond_stroke = if unreached {
            faded
        } else if on_path {
            lit
        } else {
            plain
        };
        painter.add(Shape::convex_polygon(hexagon, CARD, cond_stroke));
        let text_color = if dead { MUTED } else { TEXT };
        painter.galley(Pos2::new(cond.center().x, cy - galley.size().y / 2.0), galley, text_color);
        let about: Vec<String> = b
            .when
            .iter()
            .filter(|t| !t.always())
            .map(|t| format!("{}: {}", t.label(), t.cond.about()))
            .collect();
        let cond_hint = if about.is_empty() {
            "Условия нет: блок пробует действие всегда.".into()
        } else {
            about.join("\n")
        };
        let dead_hint = if unreached {
            "\nСюда очередь не доходит: блок выше срабатывает всегда."
        } else if off {
            "\nБлок выключен: одно из условий — «никогда». Мутация может включить его снова."
        } else if tried {
            "\nЭтот тик: условие выполнено, но сделать не вышло — решал следующий блок."
        } else if skipped {
            "\nЭтот тик: пропущена — выше уже сработала установка того же рода."
        } else {
            ""
        };
        ui.interact(cond, ui.id().with(("условие", i)), Sense::hover())
            .on_hover_text(format!("{cond_hint}{dead_hint}"));

        // «да» → the action
        let yes = if decided || applied {
            lit
        } else if tried {
            tried_stroke
        } else if dead {
            faded
        } else {
            plain
        };
        painter.arrow(Pos2::new(cond.right(), cy), Vec2::new(GAP - 2.0, 0.0), yes);
        painter.text(
            Pos2::new(cond.right() + GAP / 2.0, cy - 4.0),
            Align2::CENTER_BOTTOM,
            "да",
            FontId::proportional(12.0),
            if dead { MUTED } else { yes.color },
        );
        let color = action_color(b.action);
        let (fill, border) = if dead {
            (CARD, faded)
        } else if decided || applied {
            (color.gamma_multiply(0.35), lit)
        } else if tried {
            (color.gamma_multiply(0.18), tried_stroke)
        } else {
            (color.gamma_multiply(0.18), Stroke::new(1.3, color))
        };
        // a setting is a pill: it does not end the tick's way
        let rounding = if setting { h.min(40.0) / 2.0 } else { 5.0 };
        painter.rect(action, rounding, fill, border, egui::StrokeKind::Inside);
        text.paint(&painter, action);
        let mut action_hint = b.action.about().to_string();
        let args = b.args_label();
        if !args.is_empty() {
            action_hint.push_str(&format!("\nЗдесь: {args}."));
        }
        if setting {
            action_hint.push_str(
                "\nУстановка не решает ход и действует, где бы ни стояла в программе; из установок \
                 одного рода действует первая, чьё условие выполнено.",
            );
        } else if !b.action.never_fails() {
            action_hint.push_str("\nЕсли сделать нельзя — решает следующий блок.");
        }
        ui.interact(action, ui.id().with(("действие", i)), Sense::hover()).on_hover_text(action_hint);

        // «нет» («дальше» after a setting, «не вышло» after a failed action) ↓ the next row; never
        // taken past an unreached block or the one that always fires
        let never = unreached || ending == Some(i);
        let down = next - cond.bottom();
        let no = if never {
            faded
        } else if depth.is_some_and(|d| setting || i < d) {
            lit
        } else {
            plain
        };
        painter.arrow(Pos2::new(mid, cond.bottom()), Vec2::new(0.0, down - 2.0), no);
        let word = if setting {
            "дальше"
        } else if tried {
            "не вышло"
        } else {
            "нет"
        };
        painter.text(
            Pos2::new(mid + 6.0, cond.bottom() + (h - ch + ARROW_H) / 2.0),
            Align2::LEFT_CENTER,
            word,
            FontId::proportional(12.0),
            if never { MUTED } else { no.color },
        );
    }

    // never reached when a deciding block always fires, wherever it stands
    let end = Rect::from_min_size(Pos2::new(cond_x, end_top), Vec2::new(COND_W, END_H));
    let border = if path.is_some() && fired.is_none() {
        Stroke::new(2.5, ACCENT)
    } else if ending.is_some() {
        Stroke::new(1.0, LINE)
    } else {
        Stroke::new(1.3, MUTED)
    };
    painter.rect(end, 5.0, CARD, border, egui::StrokeKind::Inside);
    let galley = layout(ui, "ничего не подошло: стоит".into(), 13.0, TEXT, COND_W - 10.0);
    painter.galley(Pos2::new(end.center().x, end.center().y - galley.size().y / 2.0), galley, TEXT);
    ui.interact(end, ui.id().with("конец"), Sense::hover())
        .on_hover_text("Ни один блок не решил: существо стоит на месте, как в засаде.");
}
