//! Ink line-art: seam sulci, growth rings, co-rendered contours, capillary
//! veins, striae, suture ticks, and the dorsal keel.

use crate::carapace::{CarapaceGraph, PatternPoly, Scute};
use crate::geometry::{
    displace_polyline, dist, point_in_poly, poly_bounds, relax_polygon, Pt,
};
use crate::params::Params;
use crate::pigment::{compute_areola_focus, scale_poly_from_focus};
use crate::prng::{clamp, lerp, Rng};

#[derive(Clone, Debug)]
pub struct InkStrokes {
    pub primary_strokes: Vec<Vec<Pt>>,
    pub medium_strokes: Vec<Vec<Pt>>,
    pub fine_strokes: Vec<Vec<Pt>>,
}

const LITE_X: f64 = -0.55;
const LITE_Y: f64 = -0.84;

const BRIGHT_KINDS: [&str; 11] = [
    "wedge",
    "spot",
    "ocellus",
    "glyph",
    "ribbon",
    "netloop",
    "starray",
    "starwedge",
    "starrow",
    "starnode",
    "areola",
];

const CONTOUR_KINDS: [&str; 5] = ["wedge", "ocellus", "glyph", "netloop", "ribbon"];

#[inline]
fn or1(v: f64) -> f64 {
    if v == 0.0 || v.is_nan() {
        1.0
    } else {
        v
    }
}

fn dopts(
    step_len: f64,
    drift_amp: f64,
    jitter_amp: f64,
    seed_off: f64,
    pin_ends: bool,
) -> crate::geometry::DisplaceOpts {
    crate::geometry::DisplaceOpts {
        step_len,
        drift_amp,
        jitter_amp,
        seed_off,
        pin_ends,
    }
}

fn in_bright(pt: Pt, pps: &[PatternPoly]) -> bool {
    pps.iter().any(|pp| {
        pp.bounds.contains(pt) && BRIGHT_KINDS.contains(&pp.kind) && point_in_poly(pt, &pp.poly)
    })
}

#[allow(clippy::too_many_arguments)]
fn grow_vein(
    rng: &mut Rng,
    poly: &[Pt],
    s_idx: usize,
    sx: f64,
    sy: f64,
    ang: f64,
    steps: usize,
    fine: &mut Vec<Vec<Pt>>,
) -> Vec<Pt> {
    let mut px = sx;
    let mut py = sy;
    let mut dir = ang;
    let mut pts: Vec<Pt> = vec![[px, py]];
    let pb = poly_bounds(poly);
    for s in 0..steps {
        dir += rng.snoise(s as f64 * 0.8, ang * 3.0 + sx * 0.01, 0.0) * 0.9;
        px += dir.cos() * 7.0;
        py += dir.sin() * 7.0;
        if !(pb.contains([px, py]) && point_in_poly([px, py], poly)) {
            break;
        }
        pts.push([px, py]);
        if pts.len() > 4 && rng.rand() < 0.16 && steps - s > 3 {
            let branch = dir + if rng.rand() < 0.5 { 0.7 } else { -0.7 };
            let bpts = grow_vein(rng, poly, s_idx, px, py, branch, steps - s - 1, fine);
            if bpts.len() >= 2 {
                fine.push(displace_polyline(
                    rng,
                    &bpts,
                    (s_idx * 29 + s) as f64,
                    &dopts(3.0, 0.6, 0.3, 9.0, false),
                ));
            }
        }
    }
    pts
}

pub fn generate_ink_strokes(rng: &mut Rng, graph: &CarapaceGraph, p: &Params) -> InkStrokes {
    let mut primary: Vec<Vec<Pt>> = Vec::new();
    let mut medium: Vec<Vec<Pt>> = Vec::new();
    let mut fine: Vec<Vec<Pt>> = Vec::new();

    // 1. Primary shared seam sulci, plus a subtle etched sulcus on interior seams.
    let rim_seam_count = graph.outer_arcs.len() * 2;
    for (s, seam) in graph.all_seams.iter().enumerate() {
        let sf = s as f64;
        primary.push(displace_polyline(
            rng,
            seam,
            sf * 1.7 + 3.0,
            &dopts(3.5, 2.4, 0.7, sf, true),
        ));

        if s >= rim_seam_count && seam.len() > 4 && rng.rand() < 0.78 {
            let mut sub: Vec<Pt> = Vec::new();
            for i in 1..seam.len() - 1 {
                let pt = seam[i];
                let pn = seam[i + 1];
                let dx = pn[0] - pt[0];
                let dy = pn[1] - pt[1];
                let l = or1(dx.hypot(dy));
                let off = rng.snoise(pt[0] * 0.04, pt[1] * 0.04, sf) * 1.6;
                if rng.raw_noise(i as f64 * 0.35, sf * 0.7, 0.0) > 0.28 {
                    sub.push([pt[0] - (dy / l) * off, pt[1] + (dx / l) * off]);
                } else if sub.len() >= 2 {
                    medium.push(std::mem::take(&mut sub));
                }
            }
            if sub.len() >= 2 {
                medium.push(sub);
            }
        }
    }

    // 2. Per-scute growth rings, contours, veins, stippling, striae, and ticks.
    let star_active = p.pattern == "radiating" || p.interior == "starburst";
    let pattern_heavy = star_active
        || p.interior == "hieroglyph"
        || p.interior == "punctate"
        || p.interior == "net"
        || p.pattern == "spotted"
        || p.pattern == "marbled"
        || p.pattern == "ocellated";
    let ring_jit = if p.interior == "boss" { 0.2 } else { 0.5 };
    let drop_mul = if p.interior == "boss" { 0.3 } else { 0.72 };
    let sparse_rings = p.interior == "net" || p.interior == "punctate";
    let ring_hi = if sparse_rings { 1.0 } else { 0.94 };

    for (s_idx, sc) in graph.scutes.iter().enumerate() {
        let sf = s_idx as f64;
        let focus = sc.focus.unwrap_or_else(|| compute_areola_focus(rng, sc, p, graph.cx, graph.cy));
        let is_rim = matches!(sc.scute_type, "marginal" | "cervical" | "supracaudal");
        let mut n_rings: usize = if is_rim {
            if rng.rand() < 0.5 {
                1
            } else {
                2
            }
        } else {
            p.ring_count
        };
        if star_active
            || p.interior == "hieroglyph"
            || p.pattern == "marbled"
            || p.pattern == "spotted"
            || p.interior == "punctate"
        {
            n_rings = if is_rim { 1 } else { n_rings.min(6) };
        }
        let a_scale = if is_rim {
            p.areola_scale * 1.15
        } else {
            p.areola_scale
        };
        let poly = &sc.poly;
        let n = poly.len();
        let pps: &[PatternPoly] = &sc.pattern_polys;

        // Concentric annuli with dropouts; lit side drops out, shadow side etches.
        let ring_lo = if sparse_rings {
            0.85
        } else {
            (a_scale * 1.08).min(0.82)
        };
        for r in 1..=n_rings {
            let rf = r as f64;
            let u = rf / (n_rings as f64 + 1.0);
            let scale = lerp(ring_lo, ring_hi, u.powf(0.82));
            let r_blend = clamp((1.0 - scale) * 0.95, 0.0, 0.82);
            let ring_poly = relax_polygon(poly, r_blend, 3);
            let mut cur: Vec<Pt> = Vec::new();
            for i in 0..=n {
                let q = ring_poly[i % n];
                let mut rx = lerp(focus[0], q[0], scale);
                let mut ry = lerp(focus[1], q[1], scale);
                rx += rng.snoise(rx * 0.045, ry * 0.045, rf * 1.9 + sf) * 1.15;
                ry += rng.snoise(rx * 0.045, ry * 0.045, rf * 3.1 + sf) * 1.15;

                let gate = rng.raw_noise(i as f64 * 0.28 + rf * 2.3, sf * 1.7, rf * 0.5);
                let ldx = rx - focus[0];
                let ldy = ry - focus[1];
                let ldl = or1(ldx.hypot(ldy));
                let li = (ldx * LITE_X + ldy * LITE_Y) / ldl;
                let th = p.ring_dropout * drop_mul * clamp(1.0 + li * 0.9, 0.35, 1.8);
                if gate > th && !in_bright([rx, ry], pps) {
                    cur.push([rx, ry]);
                } else {
                    if cur.len() >= 2 {
                        medium.push(displace_polyline(
                            rng,
                            &cur,
                            (s_idx * 7 + r * 3 + i) as f64,
                            &dopts(3.0, 1.2, ring_jit, rf, true),
                        ));
                    }
                    cur.clear();
                }
            }
            if cur.len() >= 2 {
                medium.push(displace_polyline(
                    rng,
                    &cur,
                    (s_idx * 7 + r * 3) as f64,
                    &dopts(3.0, 1.2, ring_jit, rf, true),
                ));
            }
        }

        // Co-rendered contours around pigment geometry.
        let mut ci: usize = 0;
        for pp in pps.iter() {
            if pp.poly.len() < 3 {
                continue;
            }
            if pp.kind == "starray" || pp.kind == "starrow" {
                if p.star_mode == "fine" {
                    let q = &pp.poly;
                    let edges: [[Pt; 2]; 2] = if pp.kind == "starray" {
                        [[q[0], q[3]], [q[1], q[2]]]
                    } else {
                        [[q[0], q[2]], [q[1], q[2]]]
                    };
                    for e in edges {
                        if rng.raw_noise(sf * 3.1 + ci as f64, 7.7, 0.0) > 0.2 {
                            fine.push(displace_polyline(
                                rng,
                                &e,
                                (s_idx * 17 + ci) as f64,
                                &dopts(3.0, 0.9, 0.4, 5.0, true),
                            ));
                        }
                        ci += 1;
                    }
                }
                continue;
            }
            if !CONTOUR_KINDS.contains(&pp.kind) {
                continue;
            }
            let len = pp.poly.len();
            let mut cg: Vec<Pt> = Vec::new();
            for i in 0..=len {
                if rng.raw_noise(i as f64 * 0.35, sf * 2.1 + ci as f64, 4.4) > 0.30 {
                    cg.push(pp.poly[i % len]);
                } else if cg.len() >= 2 {
                    medium.push(displace_polyline(
                        rng,
                        &cg,
                        (s_idx * 17 + ci) as f64,
                        &dopts(3.0, 0.9, 0.4, 5.0, true),
                    ));
                    cg.clear();
                }
            }
            if cg.len() >= 2 {
                medium.push(displace_polyline(
                    rng,
                    &cg,
                    (s_idx * 17 + ci) as f64,
                    &dopts(3.0, 0.9, 0.4, 5.0, true),
                ));
            }
            ci += 1;
        }

        // Smooth interiors: capillary vein trees wandering out from the areola.
        if p.interior == "smooth" && !is_rim {
            let nv = rng.rand_int(4, 6) as usize;
            for vv in 0..nv {
                let vf = vv as f64;
                let a = (vf / nv as f64) * core::f64::consts::PI * 2.0 + rng.rand_range(-0.2, 0.2);
                let pts = grow_vein(rng, poly, s_idx, focus[0], focus[1], a, 12, &mut fine);
                if pts.len() >= 2 {
                    fine.push(displace_polyline(
                        rng,
                        &pts,
                        (s_idx * 31 + vv) as f64,
                        &dopts(3.0, 0.6, 0.3, 9.0, false),
                    ));
                }
            }
        }

        // Embryonic areola stippling.
        let area_poly =
            scale_poly_from_focus(rng, poly, focus, a_scale, a_scale, 0.05, sf * 19.0, None);
        let ab = poly_bounds(&area_poly);
        let stipple_count = if is_rim { 0 } else { 22 };
        for _ in 0..stipple_count {
            let sx = rng.rand_range(ab.min_x, ab.max_x);
            let sy = rng.rand_range(ab.min_y, ab.max_y);
            if point_in_poly([sx, sy], &area_poly) {
                let ang = rng.rand_range(0.0, core::f64::consts::PI * 2.0);
                let tl = rng.rand_range(1.2, 2.8);
                fine.push(vec![[sx, sy], [sx + ang.cos() * tl, sy + ang.sin() * tl]]);
            }
        }

        // Radial striae, only on plain grounds.
        if !is_rim && !pattern_heavy && p.striae_density > 0.25 {
            let stride = (2.8 - p.striae_density * 1.8).floor().max(1.0) as usize;
            for i in (0..n).step_by(stride) {
                let p_out = poly[i];
                let start_f = a_scale * rng.rand_range(0.95, 1.35);
                let t_end = rng.rand_range(0.7, 0.96);
                let p_in = [
                    lerp(focus[0], p_out[0], start_f),
                    lerp(focus[1], p_out[1], start_f),
                ];
                let sdx = p_out[0] - focus[0];
                let sdy = p_out[1] - focus[1];
                let sdl = or1(sdx.hypot(sdy));
                let striae_th = p.striae_density
                    * 0.62
                    * clamp(
                        1.0 - ((sdx * LITE_X + sdy * LITE_Y) / sdl) * 0.5,
                        0.55,
                        1.45,
                    );
                let ray_len = dist(p_in, p_out);
                if ray_len < 8.0 {
                    continue;
                }
                let steps = ((ray_len / 5.0).floor()).max(4.0) as usize;
                let fi = i as f64;
                let mut seg: Vec<Pt> = Vec::new();
                for st in 1..steps {
                    let t = t_end * st as f64 / steps as f64;
                    if rng.raw_noise(t * 5.0, fi * 0.9, sf) < striae_th {
                        let px = lerp(p_in[0], p_out[0], t) + rng.snoise(t * 4.0, fi, sf) * 0.9;
                        let py =
                            lerp(p_in[1], p_out[1], t) + rng.snoise(t * 4.0, fi + 5.0, sf) * 0.9;
                        if in_bright([px, py], pps) {
                            if seg.len() >= 2 {
                                fine.push(displace_polyline(
                                    rng,
                                    &seg,
                                    (s_idx * 11 + i) as f64,
                                    &dopts(2.5, 0.8, 0.4, sf, false),
                                ));
                            }
                            seg.clear();
                            continue;
                        }
                        seg.push([px, py]);
                    } else {
                        if seg.len() >= 2 {
                            fine.push(displace_polyline(
                                rng,
                                &seg,
                                (s_idx * 11 + i) as f64,
                                &dopts(2.5, 0.8, 0.4, sf, false),
                            ));
                        }
                        seg.clear();
                    }
                }
                if seg.len() >= 2 {
                    fine.push(displace_polyline(
                        rng,
                        &seg,
                        (s_idx * 11 + i) as f64,
                        &dopts(2.5, 0.8, 0.4, sf, false),
                    ));
                }
            }
        }

        // Suture and rim shading ticks on the shadowed half of each scute.
        let hatch_stride = if is_rim {
            7
        } else if pattern_heavy {
            5
        } else {
            3
        };
        for i in (0..n).step_by(hatch_stride) {
            let p0 = poly[i];
            let p1 = poly[(i + 1) % n];
            let mx = 0.5 * (p0[0] + p1[0]);
            let my = 0.5 * (p0[1] + p1[1]);
            if (mx - focus[0]) * LITE_X + (my - focus[1]) * LITE_Y > 0.0 {
                continue;
            }
            let lat_factor = (mx - graph.cx).abs() / graph.rx;
            if rng.raw_noise(mx * 0.03, my * 0.03, 9.2) + lat_factor * 0.35
                > 1.05 - p.hatch_density * 0.55
            {
                let sx = lerp(mx, focus[0], rng.rand_range(0.005, 0.035));
                let sy = lerp(my, focus[1], rng.rand_range(0.005, 0.035));
                let tick_frac = rng.rand_range(0.05, 0.28)
                    * (0.7 + 0.6 * rng.raw_noise(i as f64 * 0.4, sf, 3.3));
                let tx = lerp(mx, focus[0], tick_frac);
                let ty = lerp(my, focus[1], tick_frac);
                let hx = (sx + tx) / 2.0;
                let hy = (sy + ty) / 2.0;
                if in_bright([sx, sy], pps) || in_bright([hx, hy], pps) || in_bright([tx, ty], pps)
                {
                    continue;
                }
                fine.push(displace_polyline(
                    rng,
                    &[[sx, sy], [hx, hy], [tx, ty]],
                    (s_idx * 13 + i) as f64,
                    &dopts(2.5, 0.8, 0.4, 7.0, false),
                ));
            }
        }

        // Boss: dense slope hatching on the posterior facet.
        if p.interior == "boss" && !is_rim {
            for i in 0..n {
                let p0 = poly[i];
                let p1 = poly[(i + 1) % n];
                let mx = 0.5 * (p0[0] + p1[0]);
                let my = 0.5 * (p0[1] + p1[1]);
                if my < focus[1] {
                    continue;
                }
                if rng.raw_noise(mx * 0.05, my * 0.05, 11.7) > 0.42 {
                    let tx = lerp(mx, focus[0], rng.rand_range(0.10, 0.30));
                    let ty = lerp(my, focus[1], rng.rand_range(0.10, 0.30));
                    fine.push(displace_polyline(
                        rng,
                        &[[mx, my], [(mx + tx) / 2.0, (my + ty) / 2.0], [tx, ty]],
                        (s_idx * 37 + i) as f64,
                        &dopts(2.5, 0.7, 0.35, 12.0, false),
                    ));
                }
            }
        }
    }

    // 3. Longitudinal dorsal keel. Star webs own the vertebral midline.
    if p.keel_mode >= 1 && !star_active {
        let mut verts: Vec<&Scute> = graph
            .scutes
            .iter()
            .filter(|s| s.scute_type == "vertebral")
            .collect();
        verts.sort_by_key(|s| s.index);
        for v in verts {
            let b = v.bounds;
            let c = v.centroid;
            let y0 = b.min_y + b.h * 0.08;
            let y1 = b.max_y - b.h * 0.08;
            let vpps: &[PatternPoly] = &v.pattern_polys;
            let mut k_seg: Vec<Pt> = Vec::new();
            for st in 0..=10usize {
                let ky = lerp(y0, y1, st as f64 / 10.0);
                let kx = c[0] + rng.snoise(ky * 0.04, 0.0, 0.0) * 1.1;
                if !in_bright([kx, ky], vpps) {
                    k_seg.push([kx, ky]);
                } else {
                    if k_seg.len() >= 2 {
                        medium.push(displace_polyline(
                            rng,
                            &k_seg,
                            (v.index * 5 + 900 + st) as f64,
                            &dopts(4.0, 1.5, 0.5, 2.0, true),
                        ));
                    }
                    k_seg.clear();
                }
            }
            if k_seg.len() >= 2 {
                medium.push(displace_polyline(
                    rng,
                    &k_seg,
                    (v.index * 5 + 900) as f64,
                    &dopts(4.0, 1.5, 0.5, 2.0, true),
                ));
            }
            // Unilateral shadow ticks on the right flank of the ridge.
            for st in 1..10usize {
                let ky = lerp(y0, y1, st as f64 / 10.0);
                let kx = c[0] + rng.snoise(ky * 0.04, 0.0, 0.0) * 1.1;
                if in_bright([kx, ky], vpps) {
                    continue;
                }
                let tick = [
                    [kx + 0.8, ky],
                    [kx + rng.rand_range(2.5, 4.5), ky + rng.rand_range(0.8, 1.8)],
                ];
                if in_bright(tick[1], vpps) {
                    continue;
                }
                fine.push(tick.to_vec());
            }
        }
    }

    InkStrokes {
        primary_strokes: primary,
        medium_strokes: medium,
        fine_strokes: fine,
    }
}
