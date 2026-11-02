//! Watercolor pigment engine: wash layers, pattern archetypes, and areolae.
//!
//! RNG draws keep the JS evaluation order: each value
//! drawn from the RNG is bound to a local before any call that also borrows it.

use core::f64::consts::PI;

use crate::carapace::{CarapaceGraph, PatternPoly, Scute, StarTarget};
use crate::geometry::{
    centroid, dist, point_in_poly, poly_bounds, poly_edge_dist, poly_to_svg_path, pull_inside_poly,
    relax_polygon, Bounds, Dome, Pt,
};
use crate::params::Params;
use crate::prng::{clamp, hsl_css, lerp, Rng};
use crate::watercolor::watercolor_layers;

const STAR_STATIONS: usize = 5;

#[derive(Clone, Debug)]
pub struct Wash {
    pub d: String,
    pub fill: String,
    pub opacity: f64,
    pub rule: bool,
    pub pigment: bool,
}

#[inline]
fn or1(v: f64) -> f64 {
    if v == 0.0 || v.is_nan() {
        1.0
    } else {
        v
    }
}

/// JS `Math.round`: halves round toward +Infinity.
#[inline]
fn js_round(v: f64) -> i64 {
    (v + 0.5).floor() as i64
}

fn plain(d: String, fill: String, opacity: f64, pigment: bool) -> Wash {
    Wash {
        d,
        fill,
        opacity,
        rule: false,
        pigment,
    }
}

fn all_inside(poly: &[Pt], outline: &[Pt]) -> bool {
    let b = poly_bounds(outline);
    poly.iter().all(|q| b.contains(*q) && point_in_poly(*q, outline))
}

fn clip_poly(poly: &[Pt], anchor: Pt, outline: &[Pt]) -> Vec<Pt> {
    poly.iter()
        .map(|q| pull_inside_poly(*q, anchor, outline))
        .collect()
}

fn lerp_hue(h0: f64, h1: f64, t: f64) -> f64 {
    let mut d = h1 - h0;
    if d > 180.0 {
        d -= 360.0;
    }
    if d < -180.0 {
        d += 360.0;
    }
    (h0 + d * t + 360.0) % 360.0
}

fn band_fill(rng: &mut Rng, p: &Params) -> String {
    let h = p.col_acc1[0] + rng.rand_range(-6.0, 6.0);
    hsl_css(h, p.col_acc1[1].max(78.0), p.col_acc1[2].max(68.0))
}

pub fn deform_polygon(
    rng: &Rng,
    poly: &[Pt],
    iterations: usize,
    variance: f64,
    seed_off: f64,
) -> Vec<Pt> {
    let mut pts: Vec<Pt> = poly.to_vec();
    for it in 0..iterations {
        let itf = it as f64;
        let step_scale = variance * 0.65f64.powf(itf);
        let n = pts.len();
        let mut next: Vec<Pt> = Vec::with_capacity(n * 2);
        for i in 0..n {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            next.push(a);
            let mx = 0.5 * (a[0] + b[0]);
            let my = 0.5 * (a[1] + b[1]);
            let dx = b[0] - a[0];
            let dy = b[1] - a[1];
            let len = dx.hypot(dy);
            if len > 4.0 {
                let nx = -dy / len;
                let ny = dx / len;
                let nz = rng.snoise(mx * 0.03 + seed_off + itf * 7.1, my * 0.03, i as f64 * 0.3);
                let disp = nz * len * step_scale;
                next.push([mx + nx * disp, my + ny * disp]);
            }
        }
        pts = next;
    }
    pts
}

#[allow(clippy::too_many_arguments)]
pub fn scale_poly_from_focus(
    rng: &Rng,
    poly: &[Pt],
    focus: Pt,
    scale_x: f64,
    scale_y: f64,
    noise_jitter: f64,
    seed_off: f64,
    relax: Option<f64>,
) -> Vec<Pt> {
    let src: Vec<Pt> = match relax {
        None => {
            let r_amt = clamp(1.0 - scale_x, 0.0, 0.85);
            if r_amt > 0.0 {
                relax_polygon(poly, r_amt, 3)
            } else {
                poly.to_vec()
            }
        }
        Some(r) => {
            if r > 0.0 {
                relax_polygon(poly, r, 3)
            } else {
                poly.to_vec()
            }
        }
    };
    src.iter()
        .enumerate()
        .map(|(idx, p)| {
            let dx = p[0] - focus[0];
            let dy = p[1] - focus[1];
            let n = if noise_jitter != 0.0 {
                1.0 + rng.snoise(p[0] * 0.03 + seed_off, p[1] * 0.03, idx as f64 * 0.2)
                    * noise_jitter
            } else {
                1.0
            };
            [focus[0] + dx * scale_x * n, focus[1] + dy * scale_y * n]
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn make_blob(
    rng: &Rng,
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    angle: f64,
    lobes: usize,
    wobble: f64,
    seed_off: f64,
) -> Vec<Pt> {
    let ca = angle.cos();
    let sa = angle.sin();
    let mut pts: Vec<Pt> = Vec::with_capacity(lobes);
    for i in 0..lobes {
        let a = (i as f64 / lobes as f64) * PI * 2.0;
        let r = 1.0 + rng.snoise(a.cos() * 1.3 + seed_off, a.sin() * 1.3, seed_off * 0.5) * wobble;
        let lx = a.cos() * rx * r;
        let ly = a.sin() * ry * r;
        pts.push([cx + lx * ca - ly * sa, cy + lx * sa + ly * ca]);
    }
    pts
}

pub fn compute_areola_focus(
    rng: &Rng,
    scute: &Scute,
    p: &Params,
    shell_cx: f64,
    shell_cy: f64,
) -> Pt {
    let c = scute.centroid;
    let b: Bounds = scute.bounds;
    let mut f: Pt = if scute.scute_type == "vertebral" {
        [
            c[0] + rng.snoise(scute.index as f64, 1.1, 0.0) * b.w * 0.03,
            c[1] + b.h * p.areola_shift_y,
        ]
    } else if scute.scute_type == "costal" {
        [
            c[0] - scute.side as f64 * b.w * (0.10 + p.areola_shift_x * 0.45),
            c[1] + b.h * p.areola_shift_y,
        ]
    } else {
        let vx = c[0] - shell_cx;
        let vy = c[1] - shell_cy;
        let vl = or1(vx.hypot(vy));
        [c[0] + (vx / vl) * b.w * 0.08, c[1] + (vy / vl) * b.h * 0.08]
    };

    for _ in 0..12 {
        if scute.bounds.contains(f) && point_in_poly(f, &scute.poly) {
            break;
        }
        f = [0.5 * (f[0] + c[0]), 0.5 * (f[1] + c[1])];
    }

    let margin = 0.18 * b.w.min(b.h);
    let target = margin.min(scute.inradius);
    for _ in 0..40 {
        if poly_edge_dist(f, &scute.poly) >= target {
            break;
        }
        let nf = [lerp(f[0], c[0], 0.15), lerp(f[1], c[1], 0.15)];
        if !(scute.bounds.contains(nf) && point_in_poly(nf, &scute.poly)) {
            break;
        }
        f = nf;
    }
    if poly_edge_dist(f, &scute.poly) < target && scute.bounds.contains(c) && point_in_poly(c, &scute.poly) {
        f = [c[0], c[1]];
    }
    f
}

/// Push a pattern shape as watercolor: evolve it into translucent sublayers.
fn push_wc_shape(
    rng: &mut Rng,
    arr: &mut Vec<Wash>,
    dome: &Dome,
    poly: &[Pt],
    fill: &str,
    opacity: f64,
    pigment: bool,
) {
    if poly.len() < 3 {
        arr.push(plain(
            poly_to_svg_path(poly, dome),
            fill.to_string(),
            opacity,
            pigment,
        ));
        return;
    }
    let layers = watercolor_layers(poly, rng, 0, 1, 2, 1);
    for ring in &layers {
        let src: &[Pt] = if ring.len() >= 3 {
            ring.as_slice()
        } else {
            poly
        };
        arr.push(plain(
            poly_to_svg_path(src, dome),
            fill.to_string(),
            opacity * 0.55,
            pigment,
        ));
    }
}

struct TopoEdge {
    si: usize,
    pts: Vec<Pt>,
    len: f64,
    mid: Pt,
    m: i64,
    is_rim: bool,
}

/// Seam-station map for the kaleidoscopic dual web. Recovers the topological
/// seam graph from vertex identity (rounded-coordinate keys) and stores the
/// ray targets each scute shares with its neighbours.
fn build_seam_station_map(graph: &mut CarapaceGraph, p: &Params) {
    use std::collections::{HashMap, HashSet};

    let key = |q: Pt| -> (i64, i64) { (js_round(q[0] * 10.0), js_round(q[1] * 10.0)) };

    let mut incid: HashMap<(i64, i64), HashSet<usize>> = HashMap::new();
    for (si, sc) in graph.scutes.iter().enumerate() {
        for q in &sc.poly {
            incid.entry(key(*q)).or_default().insert(si);
        }
    }
    let count_at = |q: Pt| -> usize { incid.get(&key(q)).map_or(1, |s| s.len()) };

    let mut edges: Vec<TopoEdge> = Vec::new();
    for (si, sc) in graph.scutes.iter().enumerate() {
        let n = sc.poly.len();
        let is_corner: Vec<bool> = (0..n)
            .map(|i| {
                let c = count_at(sc.poly[i]);
                if c >= 3 {
                    true
                } else if c == 2 {
                    count_at(sc.poly[(i + n - 1) % n]) == 1 || count_at(sc.poly[(i + 1) % n]) == 1
                } else {
                    false
                }
            })
            .collect();
        let mut corners: Vec<usize> = (0..n).filter(|&i| is_corner[i]).collect();
        if corners.len() < 3 {
            corners = vec![0, n / 3, 2 * n / 3];
        }
        let nc = corners.len();
        for c in 0..nc {
            let a = corners[c];
            let b = corners[(c + 1) % nc];
            let mut pts: Vec<Pt> = Vec::new();
            let mut i = a;
            loop {
                pts.push(sc.poly[i]);
                if i == b {
                    break;
                }
                i = (i + 1) % n;
            }
            let mut len = 0.0;
            for k in 1..pts.len() {
                len += dist(pts[k - 1], pts[k]);
            }
            let mid = pts[pts.len() / 2];
            edges.push(TopoEdge {
                si,
                pts,
                len,
                mid,
                m: 0,
                is_rim: false,
            });
        }
    }

    let snow = p.star_mode == "snow";
    let fine = p.star_mode == "fine";
    let l_unit = if fine { 42.0 } else { 85.0 };
    let max_m: i64 = if fine { 3 } else { 1 };
    let min_m: i64 = if fine { 1 } else { 0 };
    let ne = edges.len();
    let mut new_m = vec![0i64; ne];
    let mut new_rim = vec![false; ne];
    for ei in 0..ne {
        let e = &edges[ei];
        let rim_pts = e.pts.iter().filter(|q| count_at(**q) == 1).count();
        if rim_pts == e.pts.len() {
            new_rim[ei] = true;
            continue;
        }
        let rx = 2.0 * graph.cx - e.mid[0];
        let ry = e.mid[1];
        let mut best: Option<usize> = None;
        let mut bd = f64::INFINITY;
        for oi in 0..ne {
            if oi == ei {
                continue;
            }
            let o = &edges[oi];
            if (o.mid[0] - graph.cx) * (e.mid[0] - graph.cx) >= 0.0 {
                continue;
            }
            let d = (o.mid[0] - rx).hypot(o.mid[1] - ry);
            if d < bd {
                bd = d;
                best = Some(oi);
            }
        }
        let l_sym = match best {
            Some(oi) => 0.5 * (e.len + edges[oi].len),
            None => e.len,
        };
        if snow {
            new_m[ei] = if l_sym > 72.0 { 1 } else { 0 };
            continue;
        }
        let m = js_round(l_sym / l_unit) - 1;
        new_m[ei] = m.min(max_m).max(min_m);
    }
    for (ei, e) in edges.iter_mut().enumerate() {
        e.m = new_m[ei];
        e.is_rim = new_rim[ei];
    }

    let mut targets_all: Vec<Vec<StarTarget>> = vec![Vec::new(); graph.scutes.len()];
    let mut seen: Vec<HashSet<(i64, i64)>> = vec![HashSet::new(); graph.scutes.len()];
    for e in &edges {
        let t_list = &mut targets_all[e.si];
        if seen[e.si].insert(key(e.pts[0])) {
            t_list.push(StarTarget {
                pt: e.pts[0],
                is_corner: true,
            });
        }
        if e.m > 0 && !e.is_rim {
            let mut cum = vec![0.0];
            for k in 1..e.pts.len() {
                let last = cum[k - 1];
                cum.push(last + dist(e.pts[k - 1], e.pts[k]));
            }
            let last_total = cum[cum.len() - 1];
            let total = if last_total == 0.0 { 1.0 } else { last_total };
            for j in 1..=e.m {
                let target = (j as f64 / (e.m as f64 + 1.0)) * total;
                let mut k = 1usize;
                while k < cum.len() - 1 && cum[k] < target {
                    k += 1;
                }
                let t = (target - cum[k - 1]) / (cum[k] - cum[k - 1]).max(1e-9);
                t_list.push(StarTarget {
                    pt: [
                        lerp(e.pts[k - 1][0], e.pts[k][0], t),
                        lerp(e.pts[k - 1][1], e.pts[k][1], t),
                    ],
                    is_corner: false,
                });
            }
        }
    }
    for (sc, targets) in graph.scutes.iter_mut().zip(targets_all) {
        sc.star_targets = targets;
    }
}

/// Ray-boundary intersection along an angle from focus (for chevrons).
fn cast_boundary(focus: Pt, ang: f64, poly: &[Pt]) -> Option<Pt> {
    let dx = ang.cos();
    let dy = ang.sin();
    let n = poly.len();
    let mut best: Option<Pt> = None;
    let mut best_t = f64::INFINITY;
    for i in 0..n {
        let p = poly[i];
        let q = poly[(i + 1) % n];
        let ex = q[0] - p[0];
        let ey = q[1] - p[1];
        let den = dx * ey - dy * ex;
        if den.abs() < 1e-9 {
            continue;
        }
        let t = ((p[0] - focus[0]) * ey - (p[1] - focus[1]) * ex) / den;
        let u = ((p[0] - focus[0]) * dy - (p[1] - focus[1]) * dx) / den;
        if t > 0.0 && u >= 0.0 && u <= 1.0 && t < best_t {
            best_t = t;
            best = Some([focus[0] + dx * t, focus[1] + dy * t]);
        }
    }
    best
}

fn clip_to_scute(pt: Pt, focus: Pt, poly: &[Pt]) -> Pt {
    if point_in_poly(pt, poly) {
        return pt;
    }
    let ang = (pt[1] - focus[1]).atan2(pt[0] - focus[0]);
    cast_boundary(focus, ang, poly).unwrap_or(pt)
}

fn flush_tip(pt: Pt, target: Pt, focus: Pt, poly: &[Pt]) -> Pt {
    let n = poly.len();
    let mut best: Option<(Pt, f64, f64, f64)> = None;
    let mut bd = f64::INFINITY;
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        let ex = b[0] - a[0];
        let ey = b[1] - a[1];
        let l2 = ex * ex + ey * ey;
        let tt = if l2 > 1e-12 {
            clamp(
                ((target[0] - a[0]) * ex + (target[1] - a[1]) * ey) / l2,
                0.0,
                1.0,
            )
        } else {
            0.0
        };
        let qx = a[0] + ex * tt;
        let qy = a[1] + ey * tt;
        let d = (target[0] - qx).hypot(target[1] - qy);
        if d < bd {
            bd = d;
            best = Some((a, ex, ey, l2));
        }
    }
    if let Some((a, ex, ey, l2)) = best {
        if l2 > 1e-9 {
            let tt = ((pt[0] - a[0]) * ex + (pt[1] - a[1]) * ey) / l2;
            if (0.0..=1.0).contains(&tt) {
                return [a[0] + ex * tt, a[1] + ey * tt];
            }
        }
    }
    clip_to_scute(pt, focus, poly)
}

#[allow(clippy::too_many_arguments)]
fn build_star_wedge(
    target: Pt,
    start_frac: f64,
    bow: f64,
    focus: Pt,
    poly: &[Pt],
    is_rim: bool,
    w_in: f64,
    w_out: f64,
) -> Option<Vec<Pt>> {
    let dx = target[0] - focus[0];
    let dy = target[1] - focus[1];
    let r = dx.hypot(dy);
    if r < 1e-6 {
        return None;
    }
    let nx = -(dy / r);
    let ny = dx / r;
    let mut left: Vec<Pt> = Vec::with_capacity(STAR_STATIONS + 1);
    let mut right: Vec<Pt> = Vec::with_capacity(STAR_STATIONS + 1);
    for s in 0..=STAR_STATIONS {
        let u = s as f64 / STAR_STATIONS as f64;
        let t = lerp(start_frac, 1.0, u);
        let bend = bow * r * (PI * u).sin();
        let cx = focus[0] + dx * t + nx * bend;
        let cy = focus[1] + dy * t + ny * bend;
        let mut w = lerp(w_in, w_out, u.powf(0.85));
        if is_rim {
            w *= 0.55;
        }
        let mut lp = [cx + nx * w, cy + ny * w];
        let mut rp = [cx - nx * w, cy - ny * w];
        if s == STAR_STATIONS {
            lp = flush_tip(lp, target, focus, poly);
            rp = flush_tip(rp, target, focus, poly);
        } else {
            let pb = poly_bounds(poly);
            if !(pb.contains(lp) && point_in_poly(lp, poly)) {
                lp = clip_to_scute(lp, focus, poly);
            }
            if !(pb.contains(rp) && point_in_poly(rp, poly)) {
                rp = clip_to_scute(rp, focus, poly);
            }
        }
        left.push(lp);
        right.push(rp);
    }
    right.reverse();
    left.extend(right);
    Some(left)
}

/// Evolve a centreline into a banded wash, plus its glaze and pattern record.
#[allow(clippy::too_many_arguments)]
fn paint_band(
    rng: &mut Rng,
    washes: &mut Vec<Wash>,
    pps: &mut Vec<PatternPoly>,
    dome: &Dome,
    sc: &Scute,
    p: &Params,
    focus: Pt,
    rad: f64,
    center: &[Pt],
    seed_off: f64,
) {
    if center.len() < 2 {
        return;
    }
    let n = center.len();
    let w_max = rad * rng.rand_range(0.068, 0.105);
    let mut left: Vec<Pt> = Vec::with_capacity(n);
    let mut right: Vec<Pt> = Vec::with_capacity(n);
    for i in 0..n {
        let pt = center[i];
        let q = center[(i + 1).min(n - 1)];
        let o = center[i.saturating_sub(1)];
        let tx = q[0] - o[0];
        let ty = q[1] - o[1];
        let tl = or1(tx.hypot(ty));
        let nx = -ty / tl;
        let ny = tx / tl;
        let u = i as f64 / (n - 1) as f64;
        let mut w = w_max * (0.68 + 0.32 * (PI * u).sin());
        w = w.min((poly_edge_dist(pt, &sc.poly) * 0.82).max(0.5));
        left.push(pull_inside_poly(
            [pt[0] + nx * w, pt[1] + ny * w],
            pt,
            &sc.poly,
        ));
        right.push(pull_inside_poly(
            [pt[0] - nx * w, pt[1] - ny * w],
            pt,
            &sc.poly,
        ));
    }
    right.reverse();
    let mut raw = left;
    raw.extend(right);
    let mut anchor = centroid(&raw);
    if !point_in_poly(anchor, &sc.poly) {
        anchor = focus;
    }
    let band_base = deform_polygon(rng, &raw, 1, 0.08, seed_off);
    let band_poly = clip_poly(&band_base, anchor, &sc.poly);
    let fill = band_fill(rng, p);
    washes.push(plain(
        poly_to_svg_path(&band_poly, dome),
        fill.clone(),
        0.80,
        true,
    ));
    let glaze_base = deform_polygon(rng, &band_poly, 1, 0.06, seed_off + 5.0);
    let glaze = clip_poly(&glaze_base, anchor, &sc.poly);
    washes.push(plain(poly_to_svg_path(&glaze, dome), fill, 0.28, true));
    pps.push(PatternPoly::new("glyph", band_poly));
}

/// Domain-warped vermiculations for box turtles.
#[allow(clippy::too_many_arguments)]
fn paint_vermiculations(
    rng: &mut Rng,
    washes: &mut Vec<Wash>,
    pps: &mut Vec<PatternPoly>,
    dome: &Dome,
    sc: &Scute,
    p: &Params,
    focus: Pt,
    rad: f64,
    s_idx: usize,
    keel: bool,
) {
    let sf = s_idx as f64;

    let ground_base = scale_poly_from_focus(
        rng,
        &sc.poly,
        focus,
        0.95,
        0.95,
        0.02,
        sf * 13.0,
        Some(0.15),
    );
    let ground_fill = hsl_css(
        p.col_dark[0],
        p.col_dark[1].max(45.0),
        p.col_dark[2].min(15.0),
    );
    push_wc_shape(rng, washes, dome, &ground_base, &ground_fill, 0.48, false);

    let n_bands = rng.rand_int(5, 8);
    let step_len = rad * 0.115;
    let seam_margin = (rad * 0.07).max(2.0);

    let hub_rx = rad * rng.rand_range(0.14, 0.22);
    let hub_ry = rad * rng.rand_range(0.10, 0.17);
    let hub_ang = rng.rand_range(0.0, 3.14);
    let hub_blotch = make_blob(
        rng,
        focus[0],
        focus[1],
        hub_rx,
        hub_ry,
        hub_ang,
        10,
        0.28,
        sf * 37.0,
    );
    let hub_fill = band_fill(rng, p);
    washes.push(plain(
        poly_to_svg_path(&hub_blotch, dome),
        hub_fill,
        0.78,
        true,
    ));
    pps.push(PatternPoly::new("glyph", hub_blotch));

    for rb in 0..n_bands {
        let rbf = rb as f64;
        let r0 = rad * rng.rand_range(0.16, 0.38);
        let mut ang = (rbf / n_bands as f64) * PI * 2.0 + rng.rand_range(-0.28, 0.28);
        let mut px = focus[0] + ang.cos() * r0;
        let mut py = focus[1] + ang.sin() * r0;
        if !(sc.bounds.contains([px, py]) && point_in_poly([px, py], &sc.poly)) {
            continue;
        }
        let mut c: Vec<Pt> = vec![[px, py]];
        let hook_sign =
            (if rb % 2 == 0 { 1.0 } else { -1.0 }) * (if s_idx % 2 == 0 { 1.0 } else { -1.0 });
        let n_steps = rng.rand_int(5, 8);
        for st in 0..n_steps {
            let curl = rng.snoise(
                px * 0.036 + sf * 1.7,
                py * 0.036 + rbf * 2.3,
                st as f64 * 0.45,
            ) * 0.48;
            ang += curl
                + if st >= n_steps - 2 {
                    hook_sign * 0.38
                } else {
                    0.0
                };
            let nx = px + ang.cos() * step_len;
            let ny = py + ang.sin() * step_len;
            if !(sc.bounds.contains([nx, ny]) && point_in_poly([nx, ny], &sc.poly))
                || poly_edge_dist([nx, ny], &sc.poly) < seam_margin
            {
                break;
            }
            px = nx;
            py = ny;
            c.push([px, py]);
        }
        paint_band(
            rng,
            washes,
            pps,
            dome,
            sc,
            p,
            focus,
            rad,
            &c,
            sf * 61.0 + rbf * 7.0,
        );

        if c.len() >= 4 && rng.rand() < 0.45 {
            let mi = c.len() / 2;
            let mp = c[mi];
            let prev = c[mi.saturating_sub(1)];
            let next = c[(mi + 1).min(c.len() - 1)];
            let tangent = (next[1] - prev[1]).atan2(next[0] - prev[0]);
            let fork_ang = tangent + if rng.rand() < 0.5 { -0.75 } else { 0.75 };
            let fork_steps = rng.rand_int(3, 4);
            let mut fx = mp[0];
            let mut fy = mp[1];
            let mut f: Vec<Pt> = vec![[fx, fy]];
            for _ in 0..fork_steps {
                let nx = fx + fork_ang.cos() * step_len;
                let ny = fy + fork_ang.sin() * step_len;
                if !(sc.bounds.contains([nx, ny]) && point_in_poly([nx, ny], &sc.poly))
                    || poly_edge_dist([nx, ny], &sc.poly) < seam_margin
                {
                    break;
                }
                fx = nx;
                fy = ny;
                f.push([fx, fy]);
            }
            paint_band(
                rng,
                washes,
                pps,
                dome,
                sc,
                p,
                focus,
                rad,
                &f,
                sf * 71.0 + rbf * 13.0,
            );
        }
    }

    if keel && sc.scute_type == "vertebral" {
        let b = sc.bounds;
        let mut ya = b.min_y + b.h * 0.10;
        let mut yb = b.max_y - b.h * 0.10;
        for _ in 0..24 {
            if sc.bounds.contains([focus[0], ya]) && point_in_poly([focus[0], ya], &sc.poly) {
                break;
            }
            ya = lerp(ya, focus[1], 0.12);
        }
        for _ in 0..24 {
            if sc.bounds.contains([focus[0], yb]) && point_in_poly([focus[0], yb], &sc.poly) {
                break;
            }
            yb = lerp(yb, focus[1], 0.12);
        }
        if yb - ya > 6.0 {
            let mut k_pts: Vec<Pt> = Vec::new();
            for k in 0..=6 {
                let ky = lerp(ya, yb, k as f64 / 6.0);
                let kx = focus[0] + rng.snoise(ky * 0.04, sf * 2.1, 0.0) * 2.2;
                if sc.bounds.contains([kx, ky]) && point_in_poly([kx, ky], &sc.poly) {
                    k_pts.push([kx, ky]);
                }
            }
            paint_band(
                rng,
                washes,
                pps,
                dome,
                sc,
                p,
                focus,
                rad,
                &k_pts,
                sf * 83.0 + 19.0,
            );
        }
    }
}

pub fn generate_watercolor_layers(
    rng: &mut Rng,
    graph: &mut CarapaceGraph,
    p: &Params,
    dome: &Dome,
) -> Vec<Wash> {
    let mut washes: Vec<Wash> = Vec::new();
    let cx = graph.cx;
    let cy = graph.cy;
    let rx = graph.rx;
    let ry = graph.ry;

    // 1. Soft ambient ground wash under the shell.
    for k in 0..2 {
        let kf = k as f64;
        let ambient = scale_poly_from_focus(
            rng,
            &graph.full_outline,
            [cx, cy + 6.0],
            1.015 + kf * 0.012,
            1.01 + kf * 0.01,
            0.015,
            kf * 19.0,
            None,
        );
        washes.push(plain(
            poly_to_svg_path(&ambient, dome),
            hsl_css(p.col_base[0], p.col_base[1] * 0.45, 76.0 - kf * 4.0),
            0.09,
            false,
        ));
    }

    // 1b. Full-shell base tint so inter-scute gaps never read paper-white.
    washes.push(plain(
        poly_to_svg_path(&graph.full_outline, dome),
        hsl_css(p.col_base[0], p.col_base[1] * 0.5, 72.0),
        0.22,
        false,
    ));

    // 1c. Whole-shell tonal field: soft blotches inside the outline.
    if rng.rand() < 0.45 {
        let bb = graph.full_bounds;
        let n_g = rng.rand_int(3, 6);
        for g in 0..n_g {
            let gf = g as f64;
            let gx = rng.rand_range(bb.min_x, bb.max_x);
            let gy = rng.rand_range(bb.min_y, bb.max_y);
            if !(graph.full_bounds.contains([gx, gy]) && point_in_poly([gx, gy], &graph.full_outline)) {
                continue;
            }
            let gr = bb.w.min(bb.h) * rng.rand_range(0.10, 0.22);
            let stretch = rng.rand_range(0.5, 0.9);
            let blob_ang = rng.rand_range(0.0, 3.0);
            let mut blob = make_blob(
                rng,
                gx,
                gy,
                gr,
                gr * stretch,
                blob_ang,
                12,
                0.4,
                gf * 31.0 + 7.0,
            );
            if !all_inside(&blob, &graph.full_outline) {
                blob = blob
                    .iter()
                    .map(|q| [gx + (q[0] - gx) * 0.6, gy + (q[1] - gy) * 0.6])
                    .collect();
                if !all_inside(&blob, &graph.full_outline) {
                    continue;
                }
            }
            let col = if rng.rand() < 0.5 {
                p.col_dark
            } else if rng.rand() < 0.5 {
                p.col_acc1
            } else {
                p.col_acc2
            };
            let fill = hsl_css(col[0] + rng.rand_range(-6.0, 6.0), col[1], col[2]);
            let opacity = rng.rand_range(0.10, 0.18);
            push_wc_shape(rng, &mut washes, dome, &blob, &fill, opacity, false);
        }
    }

    // 1d. Dome light: crown highlight plus flank and gutter shadow.
    {
        let lite = hsl_css(45.0, 32.0, 88.0);
        let crown = make_blob(
            rng,
            cx - rx * 0.12,
            cy - ry * 0.28,
            rx * 0.72,
            ry * 0.62,
            -0.5,
            12,
            0.35,
            777.0,
        );
        washes.push(Wash {
            d: poly_to_svg_path(&crown, dome),
            fill: lite,
            opacity: 0.15,
            rule: false,
            pigment: true,
        });
        let shade = hsl_css(p.col_dark[0], p.col_dark[1], (p.col_dark[2] - 6.0).max(8.0));
        let flank = make_blob(
            rng,
            cx + rx * 0.88,
            cy + ry * 0.10,
            rx * 0.28,
            ry * 0.55,
            0.35,
            12,
            0.4,
            778.0,
        );
        washes.push(plain(
            poly_to_svg_path(&flank, dome),
            shade.clone(),
            0.14,
            false,
        ));
        let gutter = make_blob(
            rng,
            cx,
            cy + ry * 0.92,
            rx * 0.55,
            ry * 0.22,
            0.0,
            12,
            0.4,
            779.0,
        );
        washes.push(plain(poly_to_svg_path(&gutter, dome), shade, 0.16, false));
    }

    let star_active = p.pattern == "radiating" || p.interior == "starburst";
    if star_active {
        build_seam_station_map(graph, p);
    }

    // 2. Per-scute washes and pattern archetypes.
    for s_idx in 0..graph.scutes.len() {
        let pps = paint_scute(rng, graph, p, dome, s_idx, &mut washes);
        graph.scutes[s_idx].pattern_polys = pps;
    }

    // Smooth interior: longitudinal pigment bands down the vertebral column.
    if p.interior == "smooth" {
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut half_w = 0.0f64;
        for v in graph.scutes.iter().filter(|s| s.scute_type == "vertebral") {
            let bb = v.bounds;
            min_y = min_y.min(bb.min_y);
            max_y = max_y.max(bb.max_y);
            half_w = half_w.max(bb.w / 2.0);
        }
        let mut lanes = vec![0.0];
        if rng.rand() < 0.6 {
            lanes.push(-1.0);
            lanes.push(1.0);
        }
        for lane in lanes {
            let bx = cx + lane * half_w * 1.1;
            let bw = half_w * rng.rand_range(0.35, 0.55);
            let mut c: Vec<Pt> = Vec::with_capacity(13);
            for s in 0..=12 {
                let t = s as f64 / 12.0;
                c.push([
                    bx + rng.snoise(t * 4.0, lane * 7.0, 0.0) * 6.0,
                    lerp(min_y, max_y, t),
                ]);
            }
            let mut strip: Vec<Pt> = Vec::with_capacity(26);
            let mut right: Vec<Pt> = Vec::with_capacity(13);
            for i in 0..=12usize {
                let q = c[(i + 1).min(12)];
                let o = c[i.saturating_sub(1)];
                let tx = q[0] - o[0];
                let ty = q[1] - o[1];
                let tl = or1(tx.hypot(ty));
                let pt = c[i];
                strip.push([pt[0] - ty / tl * bw, pt[1] + tx / tl * bw]);
                right.push([pt[0] + ty / tl * bw, pt[1] - tx / tl * bw]);
            }
            right.reverse();
            strip.extend(right);
            if !all_inside(&strip, &graph.full_outline) {
                continue;
            }
            let col = if rng.rand() < 0.7 {
                p.col_dark
            } else {
                p.col_acc1
            };
            let fill = hsl_css(col[0] + rng.rand_range(-5.0, 5.0), col[1], col[2]);
            push_wc_shape(rng, &mut washes, dome, &strip, &fill, 0.22, false);
        }
    }

    washes
}

/// Washes and pattern records for one scute. Returns the scute's pattern polys.
fn paint_scute(
    rng: &mut Rng,
    graph: &mut CarapaceGraph,
    p: &Params,
    dome: &Dome,
    s_idx: usize,
    washes: &mut Vec<Wash>,
) -> Vec<PatternPoly> {
    let cx = graph.cx;
    let cy = graph.cy;
    let ry1 = or1(graph.ry);
    let sf = s_idx as f64;
    let mut pps: Vec<PatternPoly> = Vec::new();

    let focus = {
        let sc = &graph.scutes[s_idx];
        compute_areola_focus(rng, sc, p, cx, cy)
    };
    graph.scutes[s_idx].focus = Some(focus);
    let sc = &graph.scutes[s_idx];
    let b = sc.bounds;
    let rad = b.w.min(b.h) * 0.5;
    let is_rim = matches!(sc.scute_type, "marginal" | "cervical" | "supracaudal");

    // Base watercolor washes from the scute polygon itself.
    let wc = watercolor_layers(&sc.poly, rng, 1, if is_rim { 1 } else { 2 }, 2, 2);
    for (l, ring) in wc.iter().enumerate() {
        let lf = l as f64;
        let col = if l % 2 == 0 {
            if p.inverted {
                p.col_dark
            } else {
                p.col_base
            }
        } else if p.inverted {
            p.col_dark
        } else {
            p.col_sec
        };
        let rim_lift = if is_rim && p.inverted { 14.0 } else { 0.0 };
        let (fill, opacity) = if p.inverted {
            let dark_ground = clamp(col[2] + rim_lift + rng.rand_range(-3.0, 3.0), 12.0, 40.0);
            let h = col[0] + rng.rand_range(-6.0, 6.0);
            let s = clamp(col[1] + rng.rand_range(-4.0, 4.0), 8.0, 90.0);
            (hsl_css(h, s, dark_ground), rng.rand_range(0.30, 0.45))
        } else {
            let h = col[0] + rng.rand_range(-8.0, 8.0);
            let s = clamp(col[1] + rng.rand_range(-5.0, 5.0), 8.0, 90.0);
            let l_val = clamp(col[2] - lf * rng.rand_range(1.5, 4.0), 8.0, 90.0);
            (hsl_css(h, s, l_val), rng.rand_range(0.05, 0.13))
        };
        let src: &[Pt] = if ring.len() >= 3 {
            ring.as_slice()
        } else {
            sc.poly.as_slice()
        };
        washes.push(plain(poly_to_svg_path(src, dome), fill, opacity, false));
    }

    // Sigmoid radial zoning: concentric growth-zone washes with per-scute top light.
    {
        let top_light = clamp((cy - focus[1]) / ry1, -1.0, 1.0) * 4.0;
        let outer_c = p.col_dark;
        let inner_c = if p.inverted { p.col_acc2 } else { p.col_sec };
        let sig = |u: f64| 1.0 / (1.0 + (-(u - 0.55) * 8.0).exp());
        for sc2 in [1.0, 0.8, 0.62, 0.46, 0.32] {
            let t = 1.0 - sig(sc2);
            let base = scale_poly_from_focus(
                rng,
                &sc.poly,
                focus,
                sc2,
                sc2,
                0.03,
                sf * 29.0 + sc2 * 50.0,
                None,
            );
            let layer = deform_polygon(rng, &base, 1, 0.10, sf + sc2 * 20.0);
            let fill = hsl_css(
                lerp_hue(outer_c[0], inner_c[0], t),
                lerp(outer_c[1], inner_c[1], t),
                lerp(outer_c[2], inner_c[2], t) + top_light,
            );
            push_wc_shape(rng, washes, dome, &layer, &fill, 0.12, false);
        }
    }

    // Pattern archetypes.
    let star_active = p.pattern == "radiating" || p.interior == "starburst";
    if is_rim {
        let rim_ang = (focus[1] - cy).atan2(focus[0] - cx);
        if star_active {
            let rim_spot = make_blob(
                rng,
                focus[0],
                focus[1],
                rad * 0.55,
                rad * 0.42,
                rim_ang,
                10,
                0.25,
                sf * 11.0,
            );
            let rim_col = hsl_css(
                p.col_rim[0] + rng.rand_range(-5.0, 5.0),
                p.col_rim[1],
                p.col_rim[2].max(if p.inverted { 52.0 } else { 40.0 }),
            );
            push_wc_shape(rng, washes, dome, &rim_spot, &rim_col, 0.30, true);
            pps.push(PatternPoly::new("rimspot", rim_spot));
        } else {
            let pal_name = p.palette_name;
            let ocellus_rim = pal_name.starts_with("Chrysemys")
                || pal_name.starts_with("Graptemys")
                || pal_name.starts_with("Malaclemys")
                || p.pattern == "ocellated"
                || p.pattern == "concentrica";
            let drop_rim = p.pattern == "spotted" || p.interior == "punctate";
            if drop_rim {
                let n_drops = rng.rand_int(1, 3);
                let sr = rad * 0.14;
                for d in 0..n_drops {
                    let df = d as f64;
                    let nd = n_drops as f64;
                    let ang = (df / nd) * PI * 2.0 + rng.rand_range(-0.4, 0.4);
                    let rr = rad * rng.rand_range(0.02, 0.34);
                    let sx = focus[0] + ang.cos() * rr;
                    let sy = focus[1] + ang.sin() * rr;
                    if !(sc.bounds.contains([sx, sy]) && point_in_poly([sx, sy], &sc.poly))
                        || poly_edge_dist([sx, sy], &sc.poly) < sr * 0.7
                    {
                        continue;
                    }
                    let drop = make_blob(rng, sx, sy, sr, sr * 0.9, ang, 10, 0.14, sf * 31.0 + df);
                    let drop_col = hsl_css(
                        p.col_acc1[0] + rng.rand_range(-4.0, 4.0),
                        p.col_acc1[1].max(80.0),
                        p.col_acc1[2].max(70.0),
                    );
                    washes.push(plain(poly_to_svg_path(&drop, dome), drop_col, 0.82, true));
                    pps.push(PatternPoly::new("spot", drop));
                }
            } else if ocellus_rim {
                let outer = make_blob(
                    rng,
                    focus[0],
                    focus[1],
                    rad * 0.52,
                    rad * 0.34,
                    rim_ang,
                    12,
                    0.18,
                    sf * 11.0,
                );
                let hole = make_blob(
                    rng,
                    focus[0] - rim_ang.cos() * rad * 0.10,
                    focus[1] - rim_ang.sin() * rad * 0.10,
                    rad * 0.30,
                    rad * 0.16,
                    rim_ang,
                    10,
                    0.16,
                    sf * 11.0 + 3.0,
                );
                let bright = hsl_css(
                    p.col_rim[0] + rng.rand_range(-4.0, 4.0),
                    p.col_rim[1].max(78.0),
                    p.col_rim[2].max(62.0),
                );
                washes.push(Wash {
                    d: format!(
                        "{} {}",
                        poly_to_svg_path(&outer, dome),
                        poly_to_svg_path(&hole, dome)
                    ),
                    fill: bright,
                    opacity: 0.65,
                    rule: true,
                    pigment: true,
                });
                let core = make_blob(
                    rng,
                    focus[0],
                    focus[1],
                    rad * 0.15,
                    rad * 0.11,
                    rim_ang,
                    10,
                    0.16,
                    sf * 11.0 + 7.0,
                );
                let core_fill = hsl_css(
                    p.col_dark[0],
                    p.col_dark[1].max(40.0),
                    p.col_dark[2].min(16.0),
                );
                washes.push(plain(poly_to_svg_path(&core, dome), core_fill, 0.35, true));
                pps.push(PatternPoly::new("ocellus", outer));
            } else {
                let mut corner = sc.poly[0];
                let mut best = f64::NEG_INFINITY;
                for q in &sc.poly {
                    let dx = q[0] - focus[0];
                    let dy = q[1] - focus[1];
                    let dl = or1(dx.hypot(dy));
                    let outward_dot = (dx * rim_ang.cos() + dy * rim_ang.sin()) / dl;
                    let posterior = (q[1] - cy) / ry1;
                    let score = outward_dot * 0.55 + posterior * 0.85;
                    if score > best {
                        best = score;
                        corner = *q;
                    }
                }
                let tip = [
                    lerp(corner[0], focus[0], 0.72),
                    lerp(corner[1], focus[1], 0.72),
                ];
                let dx = tip[0] - corner[0];
                let dy = tip[1] - corner[1];
                let dl = or1(dx.hypot(dy));
                let nx = -dy / dl;
                let ny = dx / dl;
                let w0 = rad * 0.22;
                let w1 = rad * 0.04;
                let wedge_raw = [
                    [corner[0] + nx * w0, corner[1] + ny * w0],
                    [tip[0] + nx * w1, tip[1] + ny * w1],
                    [tip[0] - nx * w1, tip[1] - ny * w1],
                    [corner[0] - nx * w0, corner[1] - ny * w0],
                ];
                let wedge = clip_poly(&wedge_raw, focus, &sc.poly);
                let horn = hsl_css(
                    p.col_acc2[0] + rng.rand_range(-5.0, 5.0),
                    p.col_acc2[1].max(58.0),
                    p.col_acc2[2].max(48.0),
                );
                washes.push(Wash {
                    d: poly_to_svg_path(&wedge, dome),
                    fill: horn,
                    opacity: 0.55,
                    rule: false,
                    pigment: true,
                });
                pps.push(PatternPoly::new("wedge", wedge));
            }
            return pps;
        }
    }

    if star_active {
        // Kaleidoscopic dual web: tapered horn wedges between seam stations.
        if !is_rim && !p.inverted {
            let base = scale_poly_from_focus(
                rng,
                &sc.poly,
                focus,
                0.96,
                0.96,
                0.02,
                sf * 17.0,
                Some(0.12),
            );
            let ground = hsl_css(
                p.col_dark[0],
                p.col_dark[1].max(45.0),
                (p.col_dark[2] + 3.0).min(18.0),
            );
            push_wc_shape(rng, washes, dome, &base, &ground, 0.44, false);
        }
        let snow = p.star_mode == "snow";
        let fine = p.star_mode == "fine";
        let w_in = if snow {
            2.6
        } else if fine {
            1.4
        } else {
            2.0
        };
        let w_out = if snow {
            5.6
        } else if fine {
            2.3
        } else {
            4.4
        };
        let ray_fill = hsl_css(
            p.col_acc1[0] + rng.rand_range(-4.0, 4.0),
            p.col_acc1[1],
            p.col_acc1[2],
        );

        let hub_scale = if is_rim {
            0.28
        } else {
            clamp(
                p.areola_scale
                    * (if snow {
                        0.92
                    } else if fine {
                        0.72
                    } else {
                        0.85
                    }),
                0.16,
                0.30,
            )
        };
        let hub_base = scale_poly_from_focus(
            rng,
            &sc.poly,
            focus,
            hub_scale,
            hub_scale,
            0.03,
            sf * 41.0,
            Some(0.75),
        );
        let hub_poly = deform_polygon(rng, &hub_base, 1, 0.06, sf * 41.0);

        let mut angs: Vec<f64> = Vec::new();
        for (ray_idx, t) in sc.star_targets.iter().enumerate() {
            let dx = t.pt[0] - focus[0];
            let dy = t.pt[1] - focus[1];
            let r = dx.hypot(dy);
            if r < 1e-6 {
                continue;
            }
            angs.push(dy.atan2(dx));

            // Radiata: stations directly posterior to the areola become basal spurs.
            let basal_void = fine
                && !is_rim
                && !t.is_corner
                && t.pt[1] > focus[1] + rad * 0.18
                && (t.pt[0] - focus[0]).abs() < (t.pt[1] - focus[1]) * 0.9;
            let start_frac = if basal_void { 0.72 } else { 0.45 * hub_scale };
            let bow = rng.snoise(sf * 3.7, ray_idx as f64 * 2.3, 0.0) * 0.045;
            let wedge =
                match build_star_wedge(t.pt, start_frac, bow, focus, &sc.poly, is_rim, w_in, w_out)
                {
                    Some(w) => w,
                    None => continue,
                };
            washes.push(plain(
                poly_to_svg_path(&wedge, dome),
                ray_fill.clone(),
                0.85,
                true,
            ));
            let wedge_base = deform_polygon(rng, &wedge, 1, 0.05, sf * 31.0 + ray_idx as f64);
            let wedge_wc: Vec<Pt> = wedge_base
                .iter()
                .map(|q| clip_to_scute(*q, focus, &sc.poly))
                .collect();
            washes.push(plain(
                poly_to_svg_path(&wedge_wc, dome),
                ray_fill.clone(),
                0.25,
                true,
            ));
            let last = wedge[wedge.len() - 1];
            let quad = vec![
                wedge[0],
                last,
                wedge[STAR_STATIONS + 1],
                wedge[STAR_STATIONS],
            ];
            pps.push(PatternPoly::new("starwedge", wedge));
            pps.push(PatternPoly::new("starray", quad));
        }

        // Fine harp: intercalary chevrons fill angular gaps wider than 26 degrees.
        if fine && angs.len() >= 2 {
            angs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
            let n_a = angs.len();
            for gi in 0..n_a {
                let a0 = angs[gi];
                let mut a1 = angs[(gi + 1) % n_a];
                if gi == n_a - 1 {
                    a1 += PI * 2.0;
                }
                if a1 - a0 < 0.454 {
                    continue;
                }
                let hit = match cast_boundary(focus, (a0 + a1) / 2.0, &sc.poly) {
                    Some(h) => h,
                    None => continue,
                };
                if hit[1] > focus[1] + rad * 0.18
                    && (hit[0] - focus[0]).abs() < (hit[1] - focus[1]) * 0.9
                {
                    continue;
                }
                let ax = focus[0] + (hit[0] - focus[0]) * 0.45;
                let ay = focus[1] + (hit[1] - focus[1]) * 0.45;
                let bx = hit[0] - focus[0];
                let by = hit[1] - focus[1];
                let bl = or1(bx.hypot(by));
                let nx2 = -by / bl;
                let ny2 = bx / bl;
                let tri = vec![
                    [hit[0] + nx2 * 2.0, hit[1] + ny2 * 2.0],
                    [hit[0] - nx2 * 2.0, hit[1] - ny2 * 2.0],
                    [ax, ay],
                ];
                washes.push(plain(
                    poly_to_svg_path(&tri, dome),
                    ray_fill.clone(),
                    0.8,
                    true,
                ));
                pps.push(PatternPoly::new("starrow", tri));
            }
        }

        // Areola pad caps the ray bases; faint horn/terracotta core for depth.
        push_wc_shape(rng, washes, dome, &hub_poly, &ray_fill, 0.86, true);
        let core_poly = scale_poly_from_focus(
            rng,
            &hub_poly,
            focus,
            0.62,
            0.62,
            0.02,
            sf * 43.0,
            Some(0.5),
        );
        let dh = ((p.col_acc2[0] - p.col_acc1[0] + 540.0) % 360.0) - 180.0;
        let core_fill = hsl_css(
            p.col_acc1[0] + dh * 0.25,
            lerp(p.col_acc1[1], p.col_acc2[1], 0.25),
            lerp(p.col_acc1[2], p.col_acc2[2], 0.25),
        );
        push_wc_shape(rng, washes, dome, &core_poly, &core_fill, 0.26, true);
        pps.push(PatternPoly::new("areola", hub_poly));
        if is_rim {
            return pps;
        }
    } else if p.pattern == "ocellated" {
        let r_outer =
            scale_poly_from_focus(rng, &sc.poly, focus, 0.68, 0.68, 0.08, sf * 17.0, None);
        let r_mid = scale_poly_from_focus(rng, &sc.poly, focus, 0.46, 0.46, 0.09, sf * 23.0, None);
        let core_ang = rng.rand_range(0.0, 3.0);
        let r_core = make_blob(
            rng,
            focus[0],
            focus[1],
            rad * 0.26,
            rad * 0.22,
            core_ang,
            12,
            0.24,
            sf * 37.0,
        );
        let outer_base = deform_polygon(rng, &r_outer, 1, 0.14, sf);
        let d_outer = clip_poly(&outer_base, focus, &sc.poly);
        let mid_base = deform_polygon(rng, &r_mid, 1, 0.14, sf + 9.0);
        let d_mid = clip_poly(&mid_base, focus, &sc.poly);
        let d_core = clip_poly(&r_core, focus, &sc.poly);
        let outer_fill = hsl_css(
            p.col_acc1[0],
            p.col_acc1[1].max(82.0),
            p.col_acc1[2].max(72.0),
        );
        push_wc_shape(rng, washes, dome, &d_outer, &outer_fill, 0.55, true);
        let mid_fill = hsl_css(
            p.col_dark[0],
            p.col_dark[1].max(48.0),
            p.col_dark[2].min(14.0),
        );
        push_wc_shape(rng, washes, dome, &d_mid, &mid_fill, 0.42, true);
        let core_fill = hsl_css(
            p.col_acc2[0],
            p.col_acc2[1].max(74.0),
            p.col_acc2[2].max(68.0),
        );
        push_wc_shape(rng, washes, dome, &d_core, &core_fill, 0.55, true);
        pps.push(PatternPoly::new("ocellus", d_outer));
        pps.push(PatternPoly::new("ocellus", d_mid));
        pps.push(PatternPoly::new("spot", d_core));
    } else if p.pattern == "marbled" {
        for m in 0..7 {
            let mf = m as f64;
            let ang = (mf / 7.0) * PI * 2.0 + rng.rand_range(-0.32, 0.32);
            let dd = rad * rng.rand_range(0.14, 0.64);
            let bx = focus[0] + ang.cos() * dd;
            let by = focus[1] + ang.sin() * dd;
            let rw = rad * rng.rand_range(0.24, 0.46);
            let rh = rad * rng.rand_range(0.14, 0.32);
            let blotch_base = make_blob(
                rng,
                bx,
                by,
                rw,
                rh,
                ang + 0.55,
                12,
                0.34,
                sf * 19.0 + mf * 7.0,
            );
            let blotch = clip_poly(&blotch_base, focus, &sc.poly);
            let is_dark = m % 3 == 2;
            let col = if m % 3 == 0 {
                p.col_acc1
            } else if m % 3 == 1 {
                p.col_acc2
            } else {
                p.col_dark
            };
            let fill = if is_dark {
                hsl_css(
                    col[0] + rng.rand_range(-5.0, 5.0),
                    col[1].max(45.0),
                    (col[2] + 2.0).min(16.0),
                )
            } else {
                hsl_css(
                    col[0] + rng.rand_range(-6.0, 6.0),
                    col[1].max(72.0),
                    col[2].max(58.0),
                )
            };
            push_wc_shape(
                rng,
                washes,
                dome,
                &blotch,
                &fill,
                if is_dark { 0.34 } else { 0.42 },
                !is_dark,
            );
            pps.push(PatternPoly::new("blotch", blotch));
        }
    } else if p.pattern == "reticulated" {
        for w in 0..5 {
            let wf = w as f64;
            let idx = ((wf / 5.0) * sc.poly.len() as f64).floor() as usize;
            let pv = sc.poly[idx];
            let mx = lerp(focus[0], pv[0], 0.52);
            let my = lerp(focus[1], pv[1], 0.52);
            let ang = (pv[1] - focus[1]).atan2(pv[0] - focus[0]);
            let ribbon = make_blob(
                rng,
                mx,
                my,
                rad * 0.48,
                rad * 0.11,
                ang,
                12,
                0.28,
                sf * 23.0 + wf,
            );
            let fill = hsl_css(p.col_acc1[0], p.col_acc1[1], p.col_acc1[2]);
            push_wc_shape(rng, washes, dome, &ribbon, &fill, 0.33, true);
            pps.push(PatternPoly::new("ribbon", ribbon));
        }
    } else if p.pattern == "concentrica" {
        // Diamondback growth rings: wavy annular bands plus a central boss.
        let n_bands = rng.rand_int(2, 3);
        let outer_scales: Vec<f64> = if n_bands == 2 {
            vec![0.78, 0.44]
        } else {
            vec![0.82, 0.58, 0.34]
        };
        for ri in 0..n_bands {
            let rif = ri as f64;
            let outer_scale = outer_scales[ri as usize];
            let inner_scale = (outer_scale - if ri == n_bands - 1 { 0.13 } else { 0.10 }).max(0.08);
            let seed_off = sf + rif * 3.0;
            let outer_src = scale_poly_from_focus(
                rng,
                &sc.poly,
                focus,
                outer_scale,
                outer_scale,
                0.04,
                sf * 11.0 + outer_scale * 10.0,
                None,
            );
            let outer_base = deform_polygon(rng, &outer_src, 2, 0.12, seed_off);
            let outer = clip_poly(&outer_base, focus, &sc.poly);
            let inner_src = scale_poly_from_focus(
                rng,
                &sc.poly,
                focus,
                inner_scale,
                inner_scale,
                0.04,
                sf * 11.0 + inner_scale * 10.0 + 3.0,
                None,
            );
            let inner_base = deform_polygon(rng, &inner_src, 2, 0.12, seed_off);
            let inner = clip_poly(&inner_base, focus, &sc.poly);
            let is_dark = ri % 2 == 0;
            let col = if is_dark { p.col_dark } else { p.col_acc2 };
            let fill = if is_dark {
                hsl_css(col[0], col[1].max(42.0), (col[2] + 4.0).min(20.0))
            } else {
                hsl_css(col[0], col[1].max(72.0), col[2].max(68.0))
            };
            washes.push(Wash {
                d: format!(
                    "{} {}",
                    poly_to_svg_path(&outer, dome),
                    poly_to_svg_path(&inner, dome)
                ),
                fill,
                opacity: if is_dark { 0.55 } else { 0.48 },
                rule: true,
                pigment: !is_dark,
            });
            pps.push(PatternPoly::new("bandring", outer));
        }
        let boss_scale = rng.rand_range(0.14, 0.20);
        let boss_src = scale_poly_from_focus(
            rng,
            &sc.poly,
            focus,
            boss_scale,
            boss_scale,
            0.05,
            sf * 29.0 + 7.0,
            None,
        );
        let boss = deform_polygon(rng, &boss_src, 1, 0.12, sf + 5.0);
        let boss_ochre = rng.rand() < 0.55;
        let boss_col = if boss_ochre { p.col_acc2 } else { p.col_dark };
        let boss_fill = if boss_ochre {
            hsl_css(boss_col[0], boss_col[1].max(72.0), boss_col[2].max(66.0))
        } else {
            hsl_css(
                boss_col[0],
                boss_col[1].max(42.0),
                (boss_col[2] + 4.0).min(18.0),
            )
        };
        push_wc_shape(rng, washes, dome, &boss, &boss_fill, 0.55, boss_ochre);
        pps.push(PatternPoly::new("ocellus", boss));
    }

    // Spotted and flecked turtles: guttata drops or radial fleck streams.
    if !star_active && (p.pattern == "spotted" || p.interior == "punctate") {
        let fleck_mode = (p.interior == "punctate" && p.pattern != "spotted")
            || ((p.n_vert + p.n_marg) % 2 == 0 && p.interior == "punctate");
        if !fleck_mode {
            let base = scale_poly_from_focus(
                rng,
                &sc.poly,
                focus,
                0.96,
                0.96,
                0.02,
                sf * 13.0,
                Some(0.12),
            );
            let ground = hsl_css(
                p.col_dark[0],
                p.col_dark[1].max(40.0),
                p.col_dark[2].min(16.0),
            );
            push_wc_shape(rng, washes, dome, &base, &ground, 0.32, false);
            let n_spots = rng.rand_int(1, 4);
            let mut placed: Vec<(f64, f64, f64)> = Vec::new();
            for sp in 0..n_spots {
                let spf = sp as f64;
                let sr = (rad * rng.rand_range(0.055, 0.092)).max(2.6).min(6.5);
                let mut sx = focus[0];
                let mut sy = focus[1];
                let mut ok = false;
                for _ in 0..28 {
                    let aa = rng.rand_range(0.0, PI * 2.0);
                    let rr = rad * rng.rand().sqrt() * 0.92;
                    sx = focus[0] + aa.cos() * rr;
                    sy = focus[1] + aa.sin() * rr;
                    if !(sc.bounds.contains([sx, sy]) && point_in_poly([sx, sy], &sc.poly)) {
                        continue;
                    }
                    if poly_edge_dist([sx, sy], &sc.poly) < sr * 1.05 {
                        continue;
                    }
                    if placed
                        .iter()
                        .any(|q| (q.0 - sx).hypot(q.1 - sy) < (q.2 + sr) * 1.45)
                    {
                        continue;
                    }
                    ok = true;
                    break;
                }
                if !ok {
                    continue;
                }
                placed.push((sx, sy, sr));
                let ang = (sy - focus[1]).atan2(sx - focus[0]);
                let spot = make_blob(rng, sx, sy, sr, sr * 0.88, ang, 10, 0.12, sf * 43.0 + spf);
                let spot_col = hsl_css(
                    p.col_acc1[0] + rng.rand_range(-5.0, 5.0),
                    p.col_acc1[1].max(80.0),
                    p.col_acc1[2].max(70.0),
                );
                washes.push(plain(
                    poly_to_svg_path(&spot, dome),
                    spot_col.clone(),
                    0.86,
                    true,
                ));
                let glaze_base = deform_polygon(rng, &spot, 1, 0.05, sf * 43.0 + spf + 11.0);
                let glaze = clip_poly(&glaze_base, [sx, sy], &sc.poly);
                washes.push(plain(poly_to_svg_path(&glaze, dome), spot_col, 0.25, true));
                pps.push(PatternPoly::new("spot", spot));
            }
        } else {
            let n_flecks = rng.rand_int(14, 24);
            for f in 0..n_flecks {
                let ff = f as f64;
                let frx = rad * rng.rand_range(0.045, 0.075);
                let fry = rad * rng.rand_range(0.020, 0.034);
                let aa = rng.rand_range(0.0, PI * 2.0);
                let dr = rad * rng.rand_range(0.15, 0.92).powf(0.7);
                let sx = focus[0] + aa.cos() * dr;
                let sy = focus[1] + aa.sin() * dr;
                if !(sc.bounds.contains([sx, sy]) && point_in_poly([sx, sy], &sc.poly)) {
                    continue;
                }
                if poly_edge_dist([sx, sy], &sc.poly) < frx.max(fry) * 0.9 {
                    continue;
                }
                let ang = (sy - focus[1]).atan2(sx - focus[0]);
                let fleck = make_blob(rng, sx, sy, frx, fry, ang, 10, 0.14, sf * 47.0 + ff);
                let fleck_col = hsl_css(
                    p.col_acc1[0] + rng.rand_range(-8.0, 8.0),
                    (p.col_acc1[1] * 0.72).max(42.0),
                    p.col_acc1[2].max(76.0),
                );
                washes.push(plain(poly_to_svg_path(&fleck, dome), fleck_col, 0.78, true));
                pps.push(PatternPoly::new("spot", fleck));
            }
        }
    }

    // Interior-architecture overlays: vermiculations and net loops.
    if !is_rim && !star_active && p.interior == "hieroglyph" {
        paint_vermiculations(rng, washes, &mut pps, dome, sc, p, focus, rad, s_idx, true);
    }
    if !is_rim && p.interior == "net" {
        for ins in [0.74, 0.54, 0.36] {
            let loop_src = scale_poly_from_focus(
                rng,
                &sc.poly,
                focus,
                ins,
                ins,
                0.04,
                sf * 23.0 + ins * 40.0,
                None,
            );
            let loop_pts = deform_polygon(rng, &loop_src, 1, 0.07, sf + ins * 10.0);
            let d = poly_to_svg_path(&loop_pts, dome);
            let inner_src = scale_poly_from_focus(
                rng,
                &sc.poly,
                focus,
                ins - 0.07,
                ins - 0.07,
                0.04,
                sf * 23.0 + ins * 40.0 + 3.0,
                None,
            );
            let inner_loop = deform_polygon(rng, &inner_src, 1, 0.07, sf + ins * 10.0 + 3.0);
            washes.push(Wash {
                d: format!("{} {}", d, poly_to_svg_path(&inner_loop, dome)),
                fill: hsl_css(p.col_acc1[0], p.col_acc1[1], p.col_acc1[2]),
                opacity: 0.38,
                rule: true,
                pigment: false,
            });
            pps.push(PatternPoly::new("netloop", loop_pts));
        }
    }

    pps
}
