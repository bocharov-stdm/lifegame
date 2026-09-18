//! Палитра и мелочи оформления (как `app/theme.py`, тег python-final).

use eframe::egui::{self, Color32};

pub const BG: Color32 = Color32::from_rgb(18, 21, 26);
pub const PANEL: Color32 = Color32::from_rgb(27, 31, 39);
pub const CARD: Color32 = Color32::from_rgb(35, 40, 50);
pub const LINE: Color32 = Color32::from_rgb(46, 52, 64);
pub const TEXT: Color32 = Color32::from_rgb(230, 232, 235);
pub const MUTED: Color32 = Color32::from_rgb(138, 147, 163);
pub const ACCENT: Color32 = Color32::from_rgb(245, 197, 66);
pub const ACCENT_TEXT: Color32 = Color32::from_rgb(28, 24, 12);
pub const DANGER: Color32 = Color32::from_rgb(239, 99, 81);
pub const GOOD: Color32 = Color32::from_rgb(93, 211, 158);
/// Затемнение под меню.
pub const VEIL: Color32 = Color32::from_rgba_premultiplied(10, 11, 14, 190);

pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// 12345 → «12 345»: крупные числа читаются легче.
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

/// Тёмная тема всегда: мир тёмный, и светлая панель над ним режет глаз.
pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = BG;
    v.faint_bg_color = CARD;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.55);
    v.selection.stroke.color = TEXT;
    v.hyperlink_color = ACCENT;
    // Стрелки «→», тире «‒» и кружок «●» из текстов хроники в основном шрифте
    // egui отсутствуют; встроенный Hack их знает — он запасной для обычного текста.
    let mut fonts = egui::FontDefinitions::default();
    if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        list.push("Hack".into());
    }
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(8.0, 4.0);
        s.spacing.slider_width = 220.0;
    });
}

/// Главная кнопка: жёлтая, с тёмным текстом.
pub fn primary(text: &str) -> egui::Button<'_> {
    primary_rich(egui::RichText::new(text))
}

pub fn primary_rich<'a>(text: egui::RichText) -> egui::Button<'a> {
    egui::Button::new(text.color(ACCENT_TEXT).strong()).fill(ACCENT)
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
}
