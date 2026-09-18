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
};
@group(0) @binding(0) var<uniform> u: U;

// длительности анимаций, с; призрак хранится в motion.rs чуть дольше самой долгой
const GROW: f32 = 0.35;
const SHRINK: f32 = 0.25;
const STARVE: f32 = 0.5;

const PLANT: u32 = 0u;
const VEGETARIAN: u32 = 1u;
const PREDATOR: u32 = 2u;

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
};

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
    let angle = f32(flags & 0xFFFFu) / 65536.0 * 6.2831853;
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

    let p = mix(prev, pos, u.k);
    let r_px = r * u.zoom * scale * u.pixels_per_point;
    // мельче пикселя: рисуем пиксель, но с яркостью по площади
    let drawn = max(r_px, 0.7);
    alpha = alpha * min(1.0, r_px * r_px / (drawn * drawn));
    // у хищника нос торчит за тело — квадрат побольше
    let reach = select(1.0, 1.6, kind == PREDATOR);
    let half_px = drawn * reach + 1.0;
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
    return out;
}

// расстояние до треугольника (Inigo Quilez)
fn sd_triangle(p: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>) -> f32 {
    let e0 = p1 - p0;
    let e1 = p2 - p1;
    let e2 = p0 - p2;
    let v0 = p - p0;
    let v1 = p - p1;
    let v2 = p - p2;
    let pq0 = v0 - e0 * clamp(dot(v0, e0) / dot(e0, e0), 0.0, 1.0);
    let pq1 = v1 - e1 * clamp(dot(v1, e1) / dot(e1, e1), 0.0, 1.0);
    let pq2 = v2 - e2 * clamp(dot(v2, e2) / dot(e2, e2), 0.0, 1.0);
    let s = sign(e0.x * e2.y - e0.y * e2.x);
    let d = min(
        min(
            vec2(dot(pq0, pq0), s * (v0.x * e0.y - v0.y * e0.x)),
            vec2(dot(pq1, pq1), s * (v1.x * e1.y - v1.y * e1.x)),
        ),
        vec2(dot(pq2, pq2), s * (v2.x * e2.y - v2.y * e2.x)),
    );
    return -sqrt(d.x) * sign(d.y);
}

// гладкое объединение двух фигур
fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

// 1 внутри фигуры с расстоянием d, 0 снаружи, мягкий край в пиксель
fn inside(d: f32) -> f32 {
    return clamp(0.5 - d, 0.0, 1.0);
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let r = in.r_px;
    let q = in.local;
    let len = length(q);
    var d = len - r;
    let base = in.color.rgb;
    let full = in.color.a;
    // детали появляются плавно между 3 и 6 пикселями радиуса
    let detail = clamp((r - 3.0) / 3.0, 0.0, 1.0);

    var col = base;
    if in.kind == PLANT {
        // светлее к середине — растение объёмное, а не плоское пятно
        col = base * (1.0 + 0.35 * (1.0 - clamp(len / r, 0.0, 1.0)) * detail);
    } else {
        // координаты вдоль курса и поперёк
        let f = vec2(dot(q, in.dir), dot(q, vec2(-in.dir.y, in.dir.x)));
        if in.kind == PREDATOR {
            let nose = sd_triangle(f, vec2(1.55 * r, 0.0), vec2(0.2 * r, 0.8 * r), vec2(0.2 * r, -0.8 * r));
            d = mix(d, smin(d, nose, 0.25 * r), detail);
        }
        let rim_w = max(1.0, 0.16 * r);
        let within = inside(d + rim_w);
        // ядро растёт с сытостью: полный бак — светлое тело, пустой — одна оболочка
        let core_r = (r - rim_w) * sqrt(full);
        let core = inside(len - core_r);
        var rim: vec3<f32>;
        var fill: vec3<f32>;
        var far: vec3<f32>;
        if in.kind == VEGETARIAN {
            rim = base * 0.3;
            fill = mix(base * 0.6, base * 1.05, core);
            // глазок по курсу
            let eye_r = max(1.0, 0.15 * r);
            let eye = inside(length(f - vec2(0.55 * r, 0.0)) - eye_r) * clamp((r - 5.0) / 3.0, 0.0, 1.0);
            fill = mix(fill, vec3(0.95, 0.93, 0.98), eye);
            far = base * (0.55 + 0.45 * full);
        } else {
            rim = min(base * 1.3, vec3(1.0));
            fill = mix(base * 0.3, base * 0.75, core);
            // издалека хищник — яркая точка: он главный в драме, его ищут глазами
            far = rim;
        }
        col = mix(far, mix(rim, fill, within), detail);
    }
    let lum = dot(col, vec3(0.3, 0.59, 0.11)) * 0.6;
    col = mix(col, vec3(lum), in.grey);
    let a = inside(d) * in.alpha;
    return vec4(col * a, a);
}
