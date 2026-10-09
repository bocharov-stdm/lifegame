struct U {
    // where on the screen (in points, from the viewport's top left corner) the frame's origin lies
    origin: vec2<f32>,
    // the viewport's size in points
    view: vec2<f32>,
    zoom: f32,
    pixels_per_point: f32,
    // the share of the way from the previous frame to the new one
    k: f32,
    // seconds since the frame was assembled
    since: f32,
    // seconds since the program began (modulo): the «gulps» run along the proboscis continuously
    time: f32,
    // diets to highlight, a bit per diet; 0 — nobody highlighted
    highlight: u32,
    pad1: vec2<f32>,
    // the diets' colours, from the game's palette (`theme::DIET_COLORS`)
    diets: array<vec4<f32>, 4>,
};
@group(0) @binding(0) var<uniform> u: U;

// the animations' durations, s; a ghost is kept in motion.rs a little longer than the longest
const GROW: f32 = 0.35;
const SHRINK: f32 = 0.25;
const STARVE: f32 = 0.5;

const PLANT: u32 = 0u;
const TAU: f32 = 6.2831853;

// meta (motion.rs): heading 0–11 | diet 12–13 | eats 14 | ate in the previous frame 15 | view 16–17 |
// ghost 18 | starved 19 | dot 20 | direction to the food 21–27 | proboscis length 28–31
struct VOut {
    @builtin(position) clip: vec4<f32>,
    // a point of the square relative to the centre, in pixels
    @location(0) local: vec2<f32>,
    // the body's radius in pixels
    @location(1) r_px: f32,
    // RGB and fullness
    @location(2) color: vec4<f32>,
    @location(3) alpha: f32,
    @location(4) grey: f32,
    // heading: a unit vector
    @location(5) dir: vec2<f32>,
    @location(6) @interpolate(flat) kind: u32,
    @location(7) @interpolate(flat) dot: u32,
    @location(8) @interpolate(flat) diet: u32,
    // the proboscis: the direction to the food, the length past the body's edge in pixels (0 — none), the
    // phase of the «gulps»
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
// The diets told by shape as well as by colour, once the details show: a carnivore's edge has
// teeth (notches this deep, as a share of the radius), a scavenger's rim is dashed.
const TEETH: f32 = 12.0;
const TOOTH_DEPTH: f32 = 0.12;
const DASHES: f32 = 10.0;

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
        // growth: fast at the start and soft towards the end, without a bounce
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
    // smaller than a pixel: we draw a pixel, but with a brightness by area
    let drawn = max(r_px, select(0.7, HL_MIN_PX, hl == 1u));
    alpha = select(alpha * min(1.0, r_px * r_px / (drawn * drawn)), 1.0, dot);
    if hl == 1u {
        alpha = max(alpha, select(0.0, 1.0, !ghost));
    } else if hl == 2u {
        alpha = alpha * select(DIM_CREATURE, DIM_PLANT, kind == PLANT);
    }

    // The proboscis extends during the frame in which the creature began to eat and is drawn in
    // during the frame in which it stopped; only near, like the other details.
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

// 1 inside the shape at distance d, 0 outside, a soft one-pixel edge
fn inside(d: f32) -> f32 {
    return clamp(0.5 - d, 0.0, 1.0);
}

// The rim's colour by diet: herbivore, omnivore, scavenger, carnivore.
fn diet_color(diet: u32) -> vec3<f32> {
    return u.diets[min(diet, 3u)].rgb;
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
    // the details appear smoothly between 3 and 6 pixels of radius
    let detail = clamp((r - 3.0) / 3.0, 0.0, 1.0);

    var col = base;
    // the proboscis: colour and coverage, under the body
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
        // coordinates along the heading and across it
        let f = vec2(dot(q, in.dir), dot(q, vec2(-in.dir.y, in.dir.x)));
        // the angle around the body from its heading: the teeth and dashes turn with it
        let ang = atan2(f.y, f.x);
        if in.diet == 3u {
            // a triangle wave: 0 at a tooth's tip, 1 in the notch between two
            let notch = abs(fract(ang * TEETH / TAU) - 0.5) * 2.0;
            d = len - r * (1.0 - TOOTH_DEPTH * notch * detail);
        }
        let rim_w = max(1.0, 0.16 * r);
        let within = inside(d + rim_w);
        // the body is its diet's colour: a dark shell, a light core that grows with fullness (a
        // full tank — a light body, an empty one — a shell alone), a lit rim
        let hue = diet_color(in.diet);
        let core_r = (r - rim_w) * sqrt(full);
        let core = inside(len - core_r);
        var rim = mix(hue * 0.55, hue, 0.85);
        if in.diet == 2u {
            let gap = smoothstep(0.2, 0.8, 0.5 + 0.5 * cos(ang * DASHES));
            rim = mix(rim, hue * 0.3, gap);
        }
        var fill = mix(hue * 0.26, hue * 0.92, core);
        // an eye along the heading
        let eye_r = max(1.0, 0.15 * r);
        let eye = inside(length(f - vec2(0.55 * r, 0.0)) - eye_r) * clamp((r - 5.0) / 3.0, 0.0, 1.0);
        fill = mix(fill, vec3(0.95, 0.93, 0.98), eye);
        // far away, where the details fade, the diet's colour stays
        let far = hue * (0.55 + 0.45 * full);
        col = mix(far, mix(rim, fill, within), detail);

        let reach = in.proboscis.z;
        if reach > 0.0 {
            let toward = in.proboscis.xy;
            let start = r * 0.7;
            let end = r + reach;
            let along = clamp(dot(q, toward), start, end);
            let t = (along - start) / max(end - start, 0.001);
            // thinner towards the tip; the «gulps» are swellings running from the tip to the body
            var w = max(0.8, 0.13 * r) * (1.0 - 0.35 * t);
            let gulp = 1.0 - fract(in.proboscis.w * 1.7);
            w = w * (1.0 + 0.5 * exp(-pow((t - gulp) / 0.14, 2.0)));
            tube_a = inside(length(q - toward * along) - w) * detail;
            tube = hue * 0.7 + vec3(0.06) * (1.0 - t);
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
        // a soft glow fading outward rather than a hard ring
        let glow = clamp(1.0 - d / HALO_PX, 0.0, 1.0);
        let halo = glow * glow * (1.0 - inside(d)) * 0.9 * in.alpha * (1.0 - a);
        rgb = rgb + diet_color(in.diet) * halo;
        a = a + halo;
    }
    return vec4(rgb, a);
}
