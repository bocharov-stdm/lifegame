struct U {
    // где на экране (в точках, от левого верхнего угла вьюпорта) начало координат кадра
    origin: vec2<f32>,
    // размер вьюпорта в точках
    view: vec2<f32>,
    zoom: f32,
    pixels_per_point: f32,
    // доля пути от прошлого кадра к новому
    k: f32,
    // секунды с тех пор, как кадр собран
    since: f32,
    // секунды с начала программы (по модулю): «глотки» бегут по хоботку непрерывно
    time: f32,
    // diets to highlight, a bit per diet; 0 — nobody highlighted
    highlight: u32,
    pad1: vec2<f32>,
};
@group(0) @binding(0) var<uniform> u: U;

// длительности анимаций, с; призрак хранится в motion.rs чуть дольше самой долгой
const GROW: f32 = 0.35;
const SHRINK: f32 = 0.25;
const STARVE: f32 = 0.5;

const PLANT: u32 = 0u;
const TAU: f32 = 6.2831853;

// meta (motion.rs): курс 0–11 | диета 12–13 | ест 14 | ел в прошлом кадре 15 | вид 16–17 |
// призрак 18 | с голоду 19 | точка 20 | направление к еде 21–27 | длина хоботка 28–31
struct VOut {
    @builtin(position) clip: vec4<f32>,
    // точка квадрата относительно центра, в пикселях
    @location(0) local: vec2<f32>,
    // радиус тела в пикселях
    @location(1) r_px: f32,
    // RGB и сытость
    @location(2) color: vec4<f32>,
    @location(3) alpha: f32,
    @location(4) grey: f32,
    // курс: единичный вектор
    @location(5) dir: vec2<f32>,
    @location(6) @interpolate(flat) kind: u32,
    @location(7) @interpolate(flat) dot: u32,
    @location(8) @interpolate(flat) diet: u32,
    // хоботок: направление к еде, длина за краем тела в пикселях (0 — нет), фаза «глотков»
    @location(9) @interpolate(flat) proboscis: vec4<f32>,
    // highlight: 0 as usual, 1 highlighted (a halo in its diet's colour), 2 dimmed
    @location(10) @interpolate(flat) hl: u32,
};

// how much of a dimmed body stays visible while some diets are highlighted
const DIM_CREATURE: f32 = 0.2;
const DIM_PLANT: f32 = 0.45;
// a highlighted body is at least this big, px, and wears a halo this wide
const HL_MIN_PX: f32 = 3.0;
const HALO_PX: f32 = 3.0;

@vertex
fn vs_main(
    @builtin(vertex_index) vi: u32,
    @location(0) pos: vec2<f32>,
    @location(1) prev: vec2<f32>,
    @location(2) r: f32,
    @location(3) color: u32,
    @location(4) born: f32,
    @location(5) flags: u32,
) -> VOut {
    var corners = array<vec2<f32>, 6>(
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0),
        vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0),
    );
    let kind = (flags >> 16u) & 3u;
    let ghost = (flags & (1u << 18u)) != 0u;
    let starved = (flags & (1u << 19u)) != 0u;
    let angle = f32(flags & 0xFFFu) / 4096.0 * TAU;
    let diet = (flags >> 12u) & 3u;
    var hl = 0u;
    if u.highlight != 0u {
        hl = select(2u, 1u, kind != PLANT && ((u.highlight >> diet) & 1u) != 0u);
    }
    // a highlighted creature is never a far dot: it keeps its body and halo at any zoom
    let dot = (flags & (1u << 20u)) != 0u && hl != 1u;
    let feeding = (flags & (1u << 14u)) != 0u;
    let fed = (flags & (1u << 15u)) != 0u;
    let food_angle = f32((flags >> 21u) & 127u) / 128.0 * TAU;
    let reach = f32((flags >> 28u) & 15u) / 7.5;
    let age = born + u.since;

    var scale = 1.0;
    var alpha = 1.0;
    var grey = 0.0;
    if !ghost {
        // рост: быстро в начале и мягко к концу, без отскока
        let a = clamp(age / GROW, 0.0, 1.0);
        let e = 1.0 - (1.0 - a) * (1.0 - a) * (1.0 - a);
        scale = e;
        alpha = e;
    } else if starved {
        let a = clamp(age / STARVE, 0.0, 1.0);
        grey = clamp(a * 2.0, 0.0, 1.0);
        alpha = 1.0 - a;
    } else {
        let a = clamp(age / SHRINK, 0.0, 1.0);
        scale = 1.0 - a * a;
        alpha = 1.0 - a;
    }

    let p = select(mix(prev, pos, u.k), pos, dot);
    let r_px = select(r * u.zoom * scale * u.pixels_per_point, 1.0, dot);
    // мельче пикселя: рисуем пиксель, но с яркостью по площади
    let drawn = max(r_px, select(0.7, HL_MIN_PX, hl == 1u));
    alpha = select(alpha * min(1.0, r_px * r_px / (drawn * drawn)), 1.0, dot);
    if hl == 1u {
        alpha = max(alpha, select(0.0, 1.0, !ghost));
    } else if hl == 2u {
        alpha = alpha * select(DIM_CREATURE, DIM_PLANT, kind == PLANT);
    }

    // Хоботок выдвигается, пока идёт кадр, в котором существо начало есть, и втягивается в
    // кадре, где оно перестало; только вблизи, как остальные детали.
    var extend = 0.0;
    if feeding && fed {
        extend = 1.0;
    } else if feeding {
        extend = smoothstep(0.0, 1.0, u.k);
    } else if fed {
        extend = 1.0 - smoothstep(0.0, 1.0, u.k);
    }
    let near = clamp((drawn - 3.0) / 3.0, 0.0, 1.0);
    let length_px = select(0.0, reach * drawn * extend, kind != PLANT && !ghost && !dot && near > 0.0);

    let halo_px = select(0.0, HALO_PX, hl == 1u);
    let half_px =
        select(drawn + 1.0 + max(halo_px, select(0.0, length_px + max(1.5, 0.2 * drawn), length_px > 0.0)), 1.0, dot);
    let corner = corners[vi];
    let at = u.origin + p * u.zoom + corner * (half_px / u.pixels_per_point);

    var out: VOut;
    out.clip = vec4(at.x / u.view.x * 2.0 - 1.0, 1.0 - at.y / u.view.y * 2.0, 0.0, 1.0);
    out.local = corner * half_px;
    out.r_px = drawn;
    out.color = unpack4x8unorm(color);
    out.alpha = alpha;
    out.grey = grey;
    out.dir = vec2(cos(angle), sin(angle));
    out.kind = kind;
    out.dot = u32(dot);
    // a sprout has no diet: these bits carry its leaf count, 3 to 5, from its turn
    out.diet = select(diet, 3u + (flags & 0xFFFu) % 3u, kind == PLANT);
    out.proboscis = vec4(cos(food_angle), sin(food_angle), length_px, u.time);
    out.hl = hl;
    return out;
}

// 1 внутри фигуры с расстоянием d, 0 снаружи, мягкий край в пиксель
fn inside(d: f32) -> f32 {
    return clamp(0.5 - d, 0.0, 1.0);
}

// Цвет каёмки по диете: травоядный, всеядный, падальщик, мясоед.
fn diet_color(diet: u32) -> vec3<f32> {
    switch diet {
        case 0u: { return vec3(0.35, 0.80, 0.42); }
        case 1u: { return vec3(0.90, 0.78, 0.30); }
        case 2u: { return vec3(0.62, 0.52, 0.70); }
        default: { return vec3(0.92, 0.30, 0.26); }
    }
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    if in.dot != 0u {
        return vec4(in.color.rgb * in.alpha, in.alpha);
    }
    let r = in.r_px;
    let q = in.local;
    let len = length(q);
    var d = len - r;
    let base = in.color.rgb;
    let full = in.color.a;
    // детали появляются плавно между 3 и 6 пикселями радиуса
    let detail = clamp((r - 3.0) / 3.0, 0.0, 1.0);

    var col = base;
    // хоботок: цвет и покрытие, под телом
    var tube = vec3(0.0);
    var tube_a = 0.0;
    if in.kind == PLANT {
        // A sprout: 3–5 leaves turned its own way; from afar a small dark dot.
        let f = vec2(dot(q, in.dir), dot(q, vec2(-in.dir.y, in.dir.x)));
        let n = f32(in.diet);
        let ang = atan2(f.y, f.x);
        let lobe = pow(0.5 + 0.5 * cos(n * ang), 1.6);
        d = mix(len - 0.55 * r, len - r * (0.3 + 0.7 * lobe), detail);
        // lighter towards the leaf tips, a darker midrib along each leaf, a dark stem at the heart
        let out_share = clamp(len / r, 0.0, 1.0);
        col = base * mix(0.8, 1.2, lobe * out_share);
        let rib_px = len * abs(sin(n * ang * 0.5)) * 2.0 / n;
        let rib = (1.0 - smoothstep(0.0, max(0.7, 0.05 * r), rib_px)) * smoothstep(0.2 * r, 0.35 * r, len);
        col = mix(col, base * 0.55, rib * 0.7 * detail);
        col = mix(col, base * 0.6, inside(len - 0.18 * r) * detail);
    } else {
        // координаты вдоль курса и поперёк
        let f = vec2(dot(q, in.dir), dot(q, vec2(-in.dir.y, in.dir.x)));
        let rim_w = max(1.0, 0.16 * r);
        let within = inside(d + rim_w);
        // ядро растёт с сытостью: полный бак — светлое тело, пустой — одна оболочка
        let core_r = (r - rim_w) * sqrt(full);
        let core = inside(len - core_r);
        // каёмка — цвет диеты
        let rim = mix(base * 0.3, diet_color(in.diet) * 0.85, 0.8);
        var fill = mix(base * 0.6, base * 1.05, core);
        // глазок по курсу
        let eye_r = max(1.0, 0.15 * r);
        let eye = inside(length(f - vec2(0.55 * r, 0.0)) - eye_r) * clamp((r - 5.0) / 3.0, 0.0, 1.0);
        fill = mix(fill, vec3(0.95, 0.93, 0.98), eye);
        let far = base * (0.55 + 0.45 * full);
        col = mix(far, mix(rim, fill, within), detail);

        let reach = in.proboscis.z;
        if reach > 0.0 {
            let toward = in.proboscis.xy;
            let start = r * 0.7;
            let end = r + reach;
            let along = clamp(dot(q, toward), start, end);
            let t = (along - start) / max(end - start, 0.001);
            // тоньше к кончику; «глотки» — вздутия, бегущие от кончика к телу
            var w = max(0.8, 0.13 * r) * (1.0 - 0.35 * t);
            let gulp = 1.0 - fract(in.proboscis.w * 1.7);
            w = w * (1.0 + 0.5 * exp(-pow((t - gulp) / 0.14, 2.0)));
            tube_a = inside(length(q - toward * along) - w) * detail;
            tube = mix(base * 0.75, diet_color(in.diet), 0.25) + vec3(0.06) * (1.0 - t);
        }
    }
    let lum = dot(col, vec3(0.3, 0.59, 0.11)) * 0.6;
    col = mix(col, vec3(lum), in.grey);
    let body_a = inside(d) * in.alpha;
    let t_a = tube_a * in.alpha * (1.0 - body_a);
    var rgb = col * body_a + tube * t_a;
    var a = body_a + t_a;
    if in.hl == 1u {
        // a halo in the diet's colour just outside the body
        let halo = inside(d - HALO_PX) * (1.0 - inside(d)) * 0.9 * in.alpha * (1.0 - a);
        rgb = rgb + diet_color(in.diet) * halo;
        a = a + halo;
    }
    return vec4(rgb, a);
}
