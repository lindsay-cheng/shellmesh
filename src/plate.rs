//! Complete SVG plate assembly and grid layout.
//!
//! Port of `js/plate.js` plus the grid builder from `js/main.js`.

use std::fmt::Write as _;

use crate::carapace::build_carapace_graph;
use crate::geometry::{
    push_poly_svg_path, push_strokes_compound_path, Dome, Pt, Pyramid,
};
use crate::ink::generate_ink_strokes;
use crate::params::gen_params;
use crate::pigment::{generate_watercolor_layers, scale_poly_from_focus, Wash};
use crate::prng::{push_f3, Rng};

pub struct ShellOutput {
    pub svg: String,
    pub meta: String,
    pub scute_count: usize,
    pub wash_count: usize,
}

pub(crate) fn write_shell_body(
    svg: &mut String,
    seed_str: &str,
    id_suffix: Option<usize>,
) -> (String, usize, usize) {
    let mut rng = Rng::new(seed_str);
    let p = gen_params(&mut rng);
    let mut graph = build_carapace_graph(&mut rng, &p, 380.0, 412.0, 312.0);

    let pyramids: Vec<Pyramid> = graph
        .scutes
        .iter()
        .map(|sc| {
            let c = sc.centroid;
            let rad = sc.inradius;
            Pyramid::new(c[0], c[1], rad, p.pyramiding)
        })
        .collect();
    let dome = Dome::new(graph.cx, graph.cy, graph.rx, graph.ry, p.dome_k, pyramids);
    let washes = generate_watercolor_layers(&mut rng, &mut graph, &p, &dome);
    let ink = generate_ink_strokes(&mut rng, &graph, &p);

    let mut h: i32 = 0;
    for code in seed_str.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(code as i32);
    }
    let grain_seed = (h.unsigned_abs() % 89) + 1;

    let ink_outline = scale_poly_from_focus(
        &rng,
        &graph.full_outline,
        [graph.cx, graph.cy],
        1.008,
        1.008,
        0.0,
        3.0,
        None,
    );

    let ns = |base: &str| match id_suffix {
        None => base.to_string(),
        Some(i) => format!("{}-{}", base, i),
    };
    let carapace_clip_id = ns("carapace-clip");
    let ink_clip_id = ns("ink-clip");
    let grain_id = ns("grain");
    let mottle_id = ns("mottle");

    let _ = write!(svg, "  <defs>\n    <clipPath id=\"");
    svg.push_str(&carapace_clip_id);
    svg.push_str("\"><path d=\"");
    push_poly_svg_path(svg, &graph.full_outline, &dome);
    let _ = write!(svg, "\"/></clipPath>\n    <clipPath id=\"");
    svg.push_str(&ink_clip_id);
    svg.push_str("\"><path d=\"");
    push_poly_svg_path(svg, &ink_outline, &dome);
    let _ = write!(
        svg,
        "\"/></clipPath>\n    <filter id=\"{}\" x=\"0\" y=\"0\" width=\"100%\" height=\"100%\">\n      <feTurbulence type=\"fractalNoise\" baseFrequency=\"0.45\" numOctaves=\"2\" seed=\"{}\" stitchTiles=\"stitch\" result=\"n\"/>\n      <feColorMatrix in=\"n\" type=\"matrix\" values=\"0 0 0 0 0.45  0 0 0 0 0.42  0 0 0 0 0.38  0 0 0 -0.5 0.5\"/>\n    </filter>\n    <filter id=\"{}\" x=\"0\" y=\"0\" width=\"100%\" height=\"100%\">\n      <feTurbulence type=\"fractalNoise\" baseFrequency=\"0.016 0.022\" numOctaves=\"4\" seed=\"{}\" stitchTiles=\"stitch\" result=\"n\"/>\n      <feColorMatrix in=\"n\" type=\"matrix\" values=\"0 0 0 0 0.30  0 0 0 0 0.26  0 0 0 0 0.20  0 0 0 -0.45 0.45\"/>\n    </filter>\n  </defs>\n  <rect width=\"760\" height=\"880\" fill=\"#faf6ee\"/>\n  <rect width=\"760\" height=\"880\" filter=\"url(#",
        grain_id, grain_seed, mottle_id, grain_seed
    );
    svg.push_str(&mottle_id);
    let _ = write!(
        svg,
        ")\" opacity=\"0.12\"/>\n  <g id=\"watercolor-layer\" clip-path=\"url(#",
    );
    svg.push_str(&carapace_clip_id);
    let _ = write!(
        svg,
        ")\">\n    <g id=\"wc-ground\" style=\"mix-blend-mode: multiply;\">\n    "
    );
    write_wash_nodes(svg, &washes, false);
    let _ = write!(
        svg,
        "\n    </g>\n    <g id=\"wc-pigment\" style=\"mix-blend-mode: normal;\">\n    "
    );
    write_wash_nodes(svg, &washes, true);
    let _ = write!(
        svg,
        "\n    </g>\n  </g>\n  <g id=\"ink-layer\" clip-path=\"url(#",
    );
    svg.push_str(&ink_clip_id);
    let _ = write!(
        svg,
        ")\" fill=\"none\" stroke=\"#241c15\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n    <g id=\"ink-prim\" stroke-width=\"1.40\" stroke-opacity=\"0.94\">\n    "
    );
    write_trace_nodes(svg, &ink.primary_strokes, &dome);
    let _ = write!(
        svg,
        "\n    </g>\n    <g id=\"ink-med\" stroke-width=\"0.85\" stroke-opacity=\"0.82\">\n    "
    );
    write_trace_nodes(svg, &ink.medium_strokes, &dome);
    let _ = write!(
        svg,
        "\n    </g>\n    <g id=\"ink-fine\" stroke-width=\"0.55\" stroke-opacity=\"0.68\">\n    "
    );
    write_fine_batches(svg, &ink.fine_strokes, &dome);
    let _ = write!(
        svg,
        "\n    </g>\n  </g>\n  <rect width=\"760\" height=\"880\" filter=\"url(#",
    );
    svg.push_str(&grain_id);
    svg.push_str(")\" opacity=\"0.05\"/>");
    let wm = watermark_label(seed_str);
    svg.push_str("\n  <text x=\"6\" y=\"874\" font-family=\"ui-monospace, SFMono-Regular, Menlo, Consolas, monospace\" font-size=\"9.5\" fill=\"#241c15\" fill-opacity=\"0.48\" letter-spacing=\"0.02em\">");
    svg.push_str(&wm);
    svg.push_str("</text>");
    let star_active = p.pattern == "radiating" || p.interior == "starburst";
    let meta = format!(
        "{}V · {}×2C · {}×2M ({} scutes) | {} | {} / {} / {} | pattern: {}{} | palette: {}",
        p.n_vert,
        p.n_costal,
        p.n_marg,
        graph.scutes.len(),
        if p.inverted { "dark ground" } else { "light ground" },
        p.rim_morph,
        p.disc_grid,
        p.interior,
        p.pattern,
        if star_active {
            format!(" | star: {}", p.star_mode)
        } else {
            String::new()
        },
        p.palette_name
    );

    (meta, graph.scutes.len(), washes.len())
}

pub fn generate_shell_svg(seed_str: &str) -> ShellOutput {
    let mut svg = String::with_capacity(1_600_000);
    svg.push_str("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 760 880\" width=\"100%\" height=\"100%\">\n");
    let (meta, scute_count, wash_count) = write_shell_body(&mut svg, seed_str, None);
    svg.push_str("\n</svg>");
    ShellOutput {
        svg,
        meta,
        scute_count,
        wash_count,
    }
}

/// Returns `(svg, width, height)` for an `n`×`n` contact sheet.
pub fn generate_grid_svg(n: usize, seeds: &[String]) -> (String, usize, usize) {
    const CELL_W: usize = 760;
    const CELL_H: usize = 880;
    const GAP: usize = 12;
    const BG: &str = "#161514";

    let n = n.clamp(2, 4);
    let sw = CELL_W / 2;
    let sh = CELL_H / 2;
    let w = n * sw + (n + 1) * GAP;
    let h = n * sh + (n + 1) * GAP;

    let mut svg = String::with_capacity(n * n * 1_550_000 + 256);
    let _ = write!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\"><rect width=\"{w}\" height=\"{h}\" fill=\"{BG}\"/>"
    );
    for (i, seed) in seeds.iter().take(n * n).enumerate() {
        let x = GAP + (i % n) * (sw + GAP);
        let y = GAP + (i / n) * (sh + GAP);
        let _ = write!(
            svg,
            "<g data-seed=\"{}\" transform=\"translate({x},{y}) scale(0.5)\">",
            escape_attr(seed)
        );
        let _ = write_shell_body(&mut svg, seed, Some(i));
        svg.push_str("</g>\n");
    }
    svg.push_str("</svg>");
    (svg, w, h)
}

fn write_wash_nodes(svg: &mut String, washes: &[Wash], pigment: bool) {
    let mut first = true;
    for w in washes.iter().filter(|w| w.pigment == pigment) {
        if !first {
            svg.push_str("\n    ");
        }
        first = false;
        svg.push_str("<path class=\"wash\" d=\"");
        svg.push_str(&w.d);
        svg.push_str("\" fill=\"");
        svg.push_str(&w.fill);
        svg.push_str("\" fill-opacity=\"");
        push_f3(svg, w.opacity);
        if w.rule {
            svg.push_str("\" stroke=\"none\" fill-rule=\"evenodd\"/>");
        } else {
            svg.push_str("\" stroke=\"none\"/>");
        }
    }
}

fn write_trace_nodes(svg: &mut String, strokes: &[Vec<Pt>], dome: &Dome) {
    let mut first = true;
    for s in strokes {
        if s.len() < 2 {
            continue;
        }
        if !first {
            svg.push_str("\n    ");
        }
        first = false;
        svg.push_str("<path class=\"trace\" d=\"");
        push_strokes_compound_path(svg, std::slice::from_ref(s), dome);
        svg.push_str("\"/>");
    }
}

fn write_fine_batches(svg: &mut String, fine_strokes: &[Vec<Pt>], dome: &Dome) {
    for chunk in fine_strokes.chunks(30) {
        if !chunk.iter().any(|s| s.len() >= 2) {
            continue;
        }
        svg.push_str("<path class=\"trace-batch\" d=\"");
        push_strokes_compound_path(svg, chunk, dome);
        svg.push_str("\"/>\n    ");
    }
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn watermark_label(seed_str: &str) -> String {
    let s = seed_str.trim().to_lowercase();
    let truncated: String = if s.chars().count() > 32 {
        let mut t: String = s.chars().take(32).collect();
        t.push_str("...");
        t
    } else {
        s
    };
    format!("shellmesh, {}", escape_attr(&truncated))
}
