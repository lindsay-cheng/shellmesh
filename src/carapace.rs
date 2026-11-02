//! Carapace silhouette and shared-edge topological graph.
//!
//! Port of `js/carapace.js`. Geometry and RNG call order are preserved.

use crate::geometry::{
    concat_edges, make_perturbed_edge, poly_bounds, Bounds, Pt,
};
use crate::params::Params;
use crate::prng::{clamp, lerp, Rng};

#[derive(Clone, Debug)]
pub struct PatternPoly {
    pub kind: &'static str,
    pub poly: Vec<Pt>,
    /// Precomputed so `in_bright` can reject candidates in O(1).
    pub bounds: Bounds,
}

impl PatternPoly {
    pub fn new(kind: &'static str, poly: Vec<Pt>) -> Self {
        let bounds = poly_bounds(&poly);
        Self { kind, poly, bounds }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct StarTarget {
    pub pt: Pt,
    pub is_corner: bool,
}

#[derive(Clone, Debug)]
pub struct Scute {
    pub scute_type: &'static str,
    pub index: usize,
    pub side: i32,
    pub poly: Vec<Pt>,
    pub bounds: Bounds,
    pub centroid: Pt,
    pub inradius: f64,
    pub focus: Option<Pt>,
    pub pattern_polys: Vec<PatternPoly>,
    pub star_targets: Vec<StarTarget>,
}

impl Scute {
    pub fn new(scute_type: &'static str, index: usize, side: i32, poly: Vec<Pt>) -> Self {
        let bounds = poly_bounds(&poly);
        let centroid = crate::geometry::centroid(&poly);
        let inradius = crate::geometry::poly_edge_dist(centroid, &poly).max(1.0);
        Self {
            scute_type,
            index,
            side,
            poly,
            bounds,
            centroid,
            inradius,
            focus: None,
            pattern_polys: Vec::new(),
            star_targets: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CarapaceGraph {
    pub scutes: Vec<Scute>,
    pub all_seams: Vec<Vec<Pt>>,
    pub outer_arcs: Vec<Vec<Pt>>,
    pub full_outline: Vec<Pt>,
    pub full_bounds: Bounds,
    pub cx: f64,
    pub cy: f64,
    pub rx: f64,
    pub ry: f64,
}

#[derive(Clone, Copy)]
struct Event {
    t: f64,
    t_r: Option<f64>,
    t_l: Option<f64>,
    kind: u8, // 1 = V, 2 = C
    idx: usize,
}

#[derive(Clone, Copy)]
struct ChainPt {
    pt: Pt,
    kind: u8, // 0 = START, 1 = V, 2 = C, 3 = J
    idx: usize,
}

#[allow(clippy::too_many_arguments)]
fn shell_radius(
    rng: &Rng,
    p: &Params,
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    alpha: f64,
    is_inner: bool,
) -> Pt {
    let u = alpha.sin();
    let v = -alpha.cos();
    let n = p.boxiness;
    let super_r = (u.abs().powf(n) + v.abs().powf(n)).powf(-1.0 / n);
    let taper = 1.0 + p.pyriform * v - 0.035 * v * v;

    let ant_notch = p.nuchal_notch * (-(alpha / 2.0).sin().powi(2) / 0.0045).exp();

    // Low-order harmonics: waist pinches flanks, hip flare widens rear skirt.
    let harm = 1.0 + p.waist * (2.0 * alpha).cos() + p.hip_flare * (3.0 * alpha).cos();

    // Caudal notch/cleft at the rear center (supracaudal junction).
    let d_rear = (alpha - core::f64::consts::PI)
        .abs()
        .min(2.0 * core::f64::consts::PI - (alpha - core::f64::consts::PI).abs());
    let caudal = -p.caudal_notch * (-(d_rear / 0.18).powi(2)).exp();

    let cx_n = alpha.cos() * 1.4;
    let sy_n = alpha.sin() * 1.4;
    let warp = rng.snoise(cx_n + 3.1, sy_n + 3.1, if is_inner { 4.2 } else { 1.1 }) * 0.022;
    let asym_w = rng.snoise(cx_n * 0.7, sy_n * 0.7, 8.8) * p.asym;

    // Rim collar width can vary with angle (flared skirt: tight bridge, deep rear).
    let mut rim_w = p.rim_width;
    if is_inner && p.rim_morph == "flared_skirt" {
        rim_w = p.rim_width * (0.55 + 1.15 * (alpha / 2.0).sin().powi(2));
    }
    let r_scale = if is_inner {
        1.0 - rim_w * (1.0 + 0.14 * alpha.cos()) - 0.015 * (alpha / 2.0).sin().powi(2)
    } else {
        1.0
    };

    let r = super_r * (1.0 + ant_notch + caudal + harm - 1.0 + warp + asym_w) * r_scale;
    [cx + u * rx * taper * r, cy + v * ry * r]
}

fn build_inner_arc<'a>(
    segs: &'a [Vec<Pt>],
    k_start: usize,
    k_end: usize,
    k_count: usize,
) -> Vec<(&'a [Pt], bool)> {
    let mut edges = Vec::new();
    let mut k = k_start;
    while k != k_end {
        edges.push((segs[k].as_slice(), false));
        k = (k + 1) % k_count;
    }
    edges
}

pub fn build_carapace_graph(
    rng: &mut Rng,
    p: &Params,
    cx: f64,
    cy: f64,
    ry: f64,
) -> CarapaceGraph {
    let rx = ry * p.aspect;

    // 1. Marginal angular stations around [0, 2*PI].
    let d_cerv = 0.048 * core::f64::consts::PI;
    let d_sup = if p.fused_supracaudal {
        0.048 * core::f64::consts::PI
    } else {
        0.0
    };
    let mut marg_alphas = vec![d_cerv];
    let span_side = core::f64::consts::PI - d_cerv - d_sup;
    for i in 1..=p.n_marg {
        let t = i as f64 / p.n_marg as f64;
        let a = d_cerv
            + span_side
                * (t + 0.012 * (core::f64::consts::PI * t * 2.0).sin());
        marg_alphas.push(a + rng.snoise(i as f64 * 0.7, 1.2, 0.0) * 0.008);
    }
    if p.fused_supracaudal {
        marg_alphas.push(core::f64::consts::PI + d_sup);
    }
    for i in (1..p.n_marg).rev() {
        let t = i as f64 / p.n_marg as f64;
        let a = 2.0 * core::f64::consts::PI
            - (d_cerv + span_side * (t + 0.012 * (core::f64::consts::PI * t * 2.0).sin()));
        marg_alphas.push(a + rng.snoise(i as f64 * 0.7, 5.4, 0.0) * 0.008);
    }
    marg_alphas.push(2.0 * core::f64::consts::PI - d_cerv);

    let k_count = marg_alphas.len();
    let outer_nodes: Vec<Pt> = marg_alphas
        .iter()
        .map(|&a| shell_radius(rng, p, cx, cy, rx, ry, a, false))
        .collect();
    let inner_marg_nodes: Vec<Pt> = marg_alphas
        .iter()
        .map(|&a| shell_radius(rng, p, cx, cy, rx, ry, a, true))
        .collect();

    // Build outer rim arcs and radial marginal seams. Rim ornament is
    // scute-locked on per-scute u in [0,1].
    let mut outer_arcs: Vec<Vec<Pt>> = Vec::with_capacity(k_count);
    let mut radial_seams: Vec<Vec<Pt>> = Vec::with_capacity(k_count);
    for k in 0..k_count {
        let a0 = marg_alphas[k];
        let a1 = if k == k_count - 1 {
            marg_alphas[0] + 2.0 * core::f64::consts::PI
        } else {
            marg_alphas[k + 1]
        };
        let mid_a = (a0 + a1) / 2.0;
        let mid_norm =
            ((mid_a % (2.0 * core::f64::consts::PI)) + 2.0 * core::f64::consts::PI)
                % (2.0 * core::f64::consts::PI);
        let is_posterior = mid_norm > 0.55 * core::f64::consts::PI
            && mid_norm < 1.45 * core::f64::consts::PI;
        let steps = if p.rim_morph == "sawback" && is_posterior {
            16
        } else {
            12
        };

        let mut arc = Vec::with_capacity(steps + 1);
        for s in 0..=steps {
            let u = s as f64 / steps as f64;
            let pt = shell_radius(rng, p, cx, cy, rx, ry, lerp(a0, a1, u), false);
            let m = if p.rim_morph == "scalloped" {
                1.0 + (0.015 + p.serration * 0.4)
                    * (core::f64::consts::PI * u).sin().powf(0.85)
            } else if p.rim_morph == "sawback" && is_posterior {
                let e = 1.2 + p.serr_sharp * 0.5;
                // Capped blunt teeth: reads as serrated rim, never detached shards.
                1.0 + (0.024 + p.serration * 0.95).min(0.048)
                    * u.powf(1.6 * e / 1.7)
                    * (0.0_f64.max(1.0 - u)).powf(0.5)
                    / 0.45
            } else {
                // collared / narrow_rim / flared_skirt: subtle per-scute ripple.
                1.0 + p.serration * 0.3 * (core::f64::consts::PI * u).sin()
            };
            arc.push([cx + (pt[0] - cx) * m, cy + (pt[1] - cy) * m]);
        }
        arc[0] = outer_nodes[k];
        let last = arc.len() - 1;
        arc[last] = outer_nodes[(k + 1) % k_count];
        outer_arcs.push(arc);

        // Slight oblique angle on marginal sutures.
        let bow = a0.sin() * 0.08;
        radial_seams.push(make_perturbed_edge(
            rng,
            inner_marg_nodes[k],
            outer_nodes[k],
            6,
            bow,
            1.2,
            k as f64 * 3.1,
        ));
    }

    // 2. Map disc-to-rim attachment indices on the inner marginal ring.
    let idx_ant_r = 1;
    let idx_post_r = p.n_marg - 1;
    let idx_ant_l = k_count - 2;
    let idx_post_l = k_count - p.n_marg;

    let mut c_attach_r = vec![idx_ant_r];
    let mut c_attach_l = vec![idx_ant_l];
    for j in 1..p.n_costal {
        let frac = j as f64 / p.n_costal as f64;
        let ir = lerp((idx_ant_r + 1) as f64, (idx_post_r - 1) as f64, frac).round() as usize;
        let il = lerp((idx_ant_l - 1) as f64, (idx_post_l + 1) as f64, frac).round() as usize;
        c_attach_r.push(ir);
        c_attach_l.push(il);
    }
    c_attach_r.push(idx_post_r);
    c_attach_l.push(idx_post_l);

    // Inner ring segments between consecutive marginal nodes.
    let mut inner_ring_segs: Vec<Vec<Pt>> = Vec::with_capacity(k_count);
    for k in 0..k_count {
        let a0 = marg_alphas[k];
        let a1 = if k == k_count - 1 {
            marg_alphas[0] + 2.0 * core::f64::consts::PI
        } else {
            marg_alphas[k + 1]
        };
        let mut arc = Vec::with_capacity(9);
        for s in 0..=8 {
            arc.push(shell_radius(
                rng,
                p,
                cx,
                cy,
                rx,
                ry,
                lerp(a0, a1, s as f64 / 8.0),
                true,
            ));
        }
        arc[0] = inner_marg_nodes[k];
        let last = arc.len() - 1;
        arc[last] = inner_marg_nodes[(k + 1) % k_count];
        inner_ring_segs.push(arc);
    }

    // Marginal scute polygons.
    let mut scutes: Vec<Scute> = Vec::new();
    for k in 0..k_count {
        let kn = (k + 1) % k_count;
        let scute_type = if k == k_count - 1 {
            "cervical"
        } else if p.fused_supracaudal && k == p.n_marg {
            "supracaudal"
        } else {
            "marginal"
        };
        let poly = concat_edges(&[
            (inner_ring_segs[k].as_slice(), false),
            (radial_seams[kn].as_slice(), false),
            (outer_arcs[k].as_slice(), true),
            (radial_seams[k].as_slice(), true),
        ]);
        let side = if (k as f64) < k_count as f64 / 2.0 {
            1
        } else {
            -1
        };
        scutes.push(Scute::new(scute_type, k, side, poly));
    }

    // 3. Central disc: gamma-warped vertebral stations + grid-aware costal placement.
    let mut v_t = vec![0.0];
    for i in 1..p.n_vert {
        v_t.push(clamp(
            (i as f64 / p.n_vert as f64).powf(p.spine_gamma)
                + rng.snoise(i as f64, 2.1, 0.0) * 0.018,
            0.06,
            0.94,
        ));
    }
    v_t.push(1.0);

    let mut events: Vec<Event> = Vec::new();
    for i in 1..p.n_vert {
        events.push(Event {
            t: v_t[i],
            t_r: None,
            t_l: None,
            kind: 1,
            idx: i,
        });
    }
    let mut used_gap = vec![false; p.n_vert];
    for j in 1..p.n_costal {
        let gap = j.min(p.n_vert - 1);
        let (mut f_r, mut f_l);
        if p.disc_grid == "quadrate_brick" {
            f_r = rng.rand_range(0.46, 0.54);
            f_l = f_r;
        } else if p.disc_grid == "alternating_dovetail" {
            f_r = 0.40;
            f_l = 0.60;
        } else {
            f_r = rng.rand_range(0.42, 0.58);
            f_l = f_r;
        }
        if used_gap[gap] {
            f_r = (f_r + 0.15).min(0.8);
            f_l = (f_l + 0.15).min(0.8);
        }
        used_gap[gap] = true;
        let t_base = lerp(v_t[gap], v_t[gap + 1], 0.5);
        events.push(Event {
            t: t_base,
            t_r: Some(lerp(v_t[gap], v_t[gap + 1], f_r)),
            t_l: Some(lerp(v_t[gap], v_t[gap + 1], f_l)),
            kind: 2,
            idx: j,
        });
    }
    events.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(core::cmp::Ordering::Equal));
    for m in 1..events.len() {
        if events[m].t - events[m - 1].t < 0.045 {
            let mid = 0.5 * (events[m].t + events[m - 1].t);
            for is_prev in [true, false] {
                let e = if is_prev {
                    &mut events[m - 1]
                } else {
                    &mut events[m]
                };
                let target = if is_prev { mid - 0.023 } else { mid + 0.023 };
                let d = target - e.t;
                e.t += d;
                if let Some(v) = e.t_r.as_mut() {
                    *v += d;
                }
                if let Some(v) = e.t_l.as_mut() {
                    *v += d;
                }
            }
        }
    }

    let top_y = 0.5 * (inner_marg_nodes[idx_ant_r][1] + inner_marg_nodes[idx_ant_l][1]);
    let bot_y = 0.5 * (inner_marg_nodes[idx_post_r][1] + inner_marg_nodes[idx_post_l][1]);

    let mut chain_r = vec![ChainPt {
        pt: inner_marg_nodes[idx_ant_r],
        kind: 0,
        idx: 0,
    }];
    let mut chain_l = vec![ChainPt {
        pt: inner_marg_nodes[idx_ant_l],
        kind: 0,
        idx: 0,
    }];

    let zig_amp = if p.disc_grid == "alternating_dovetail" {
        0.022
    } else {
        p.zigzag
    };
    let eff_hex = if p.disc_grid == "quadrate_brick" {
        (p.hex_peak * 0.55).max(0.065)
    } else {
        p.hex_peak
    };

    for ev in &events {
        let t_r = ev.t_r.unwrap_or(ev.t);
        let t_l = ev.t_l.unwrap_or(ev.t);
        let y_r = lerp(top_y, bot_y, t_r);
        let y_l = lerp(top_y, bot_y, t_l);
        let alpha_r = ((cy - y_r) / (ry * (1.0 - p.rim_width)))
            .clamp(-0.98, 0.98)
            .acos();
        let alpha_l = ((cy - y_l) / (ry * (1.0 - p.rim_width)))
            .clamp(-0.98, 0.98)
            .acos();
        let half_w_r = (shell_radius(rng, p, cx, cy, rx, ry, alpha_r, true)[0] - cx).abs();
        let half_w_l = (shell_radius(rng, p, cx, cy, rx, ry, alpha_l, true)[0] - cx).abs();

        let offset = if ev.kind == 2 {
            eff_hex
        } else {
            -eff_hex * 0.65
        };
        let w_frac_r = p.vert_width + offset + rng.snoise(ev.t * 4.0, 1.1, 0.0) * p.asym;
        let w_frac_l = p.vert_width + offset + rng.snoise(ev.t * 4.0, 9.4, 0.0) * p.asym;

        let zig_y = if ev.kind == 1 {
            zig_amp * ry * if ev.idx % 2 == 0 { 1.0 } else { -1.0 }
        } else {
            0.0
        };
        let pt_r = [
            cx + half_w_r * w_frac_r,
            y_r + zig_y + rng.snoise(ev.t * 5.0, 3.3, 0.0) * 4.5,
        ];
        let pt_l = [
            cx - half_w_l * w_frac_l,
            y_l - zig_y + rng.snoise(ev.t * 5.0, 6.6, 0.0) * 4.5,
        ];

        chain_r.push(ChainPt {
            pt: pt_r,
            kind: ev.kind,
            idx: ev.idx,
        });
        chain_l.push(ChainPt {
            pt: pt_l,
            kind: ev.kind,
            idx: ev.idx,
        });
    }

    // Posterior junction: chains terminate interior instead of pinching to
    // single rim points. Two posteromedial seams close the last vertebral.
    let mut max_t = 0.0_f64;
    for ev in &events {
        max_t = max_t.max(ev.t);
        max_t = max_t.max(ev.t_r.unwrap_or(ev.t));
        max_t = max_t.max(ev.t_l.unwrap_or(ev.t));
    }
    let t_j = (max_t + 0.03).min(0.97);
    let y_j = lerp(top_y, bot_y, t_j);
    let alpha_j = ((cy - y_j) / (ry * (1.0 - p.rim_width)))
        .clamp(-0.98, 0.98)
        .acos();
    let half_w_j = (shell_radius(rng, p, cx, cy, rx, ry, alpha_j, true)[0] - cx).abs();
    let w_jr = p.vert_width + eff_hex * 0.3 + rng.snoise(t_j * 4.0, 1.1, 0.0) * p.asym;
    let w_jl = p.vert_width + eff_hex * 0.3 + rng.snoise(t_j * 4.0, 9.4, 0.0) * p.asym;
    let j_r = [cx + half_w_j * w_jr, y_j];
    let j_l = [cx - half_w_j * w_jl, y_j];
    chain_r.push(ChainPt {
        pt: j_r,
        kind: 3,
        idx: 0,
    });
    chain_l.push(ChainPt {
        pt: j_l,
        kind: 3,
        idx: 0,
    });

    let post_med_r = make_perturbed_edge(rng, j_r, inner_marg_nodes[idx_post_r], 8, 0.0, 2.0, 77.0);
    let post_med_l = make_perturbed_edge(rng, j_l, inner_marg_nodes[idx_post_l], 8, 0.0, 2.0, 78.0);

    // Build shared chain edges.
    let mut segs_r: Vec<Vec<Pt>> = Vec::new();
    let mut segs_l: Vec<Vec<Pt>> = Vec::new();
    for i in 0..chain_r.len() - 1 {
        segs_r.push(make_perturbed_edge(
            rng,
            chain_r[i].pt,
            chain_r[i + 1].pt,
            8,
            0.03,
            2.2,
            i as f64 * 1.7,
        ));
        segs_l.push(make_perturbed_edge(
            rng,
            chain_l[i].pt,
            chain_l[i + 1].pt,
            8,
            -0.03,
            2.2,
            i as f64 * 2.9 + 10.0,
        ));
    }

    // Build inter-vertebral transverse seams.
    let mut v_seams: std::collections::BTreeMap<usize, Vec<Pt>> =
        std::collections::BTreeMap::new();
    let mut v_chain_indices = vec![0usize];
    for i in 1..chain_r.len() - 1 {
        if chain_r[i].kind == 1 {
            let vi = chain_r[i].idx;
            let bow = if (vi as f64) < p.n_vert as f64 / 2.0 {
                -0.05
            } else {
                0.04
            };
            v_seams.insert(
                vi,
                make_perturbed_edge(
                    rng,
                    chain_l[i].pt,
                    chain_r[i].pt,
                    14,
                    bow,
                    2.8,
                    vi as f64 * 5.3,
                ),
            );
            v_chain_indices.push(i);
        }
    }
    v_chain_indices.push(chain_r.len() - 1);

    // Build inter-costal transverse seams.
    let mut c_seams_r: std::collections::BTreeMap<usize, Vec<Pt>> =
        std::collections::BTreeMap::new();
    let mut c_seams_l: std::collections::BTreeMap<usize, Vec<Pt>> =
        std::collections::BTreeMap::new();
    let mut c_chain_indices = vec![0usize];
    for i in 1..chain_r.len() - 1 {
        if chain_r[i].kind == 2 {
            let cj = chain_r[i].idx;
            let rim_r = inner_marg_nodes[c_attach_r[cj]];
            let rim_l = inner_marg_nodes[c_attach_l[cj]];
            // Keep posterior seams from bowing across their neighbor.
            let bow = clamp(
                (cj as f64 - p.n_costal as f64 * 0.45) * 0.035,
                -0.03,
                0.03,
            );
            c_seams_r.insert(
                cj,
                make_perturbed_edge(rng, chain_r[i].pt, rim_r, 14, -bow, 3.0, cj as f64 * 7.1),
            );
            c_seams_l.insert(
                cj,
                make_perturbed_edge(
                    rng,
                    chain_l[i].pt,
                    rim_l,
                    14,
                    bow,
                    3.0,
                    cj as f64 * 8.3 + 20.0,
                ),
            );
            c_chain_indices.push(i);
        }
    }
    c_chain_indices.push(chain_r.len() - 1);

    // Assemble vertebral scute polygons.
    for v in 0..p.n_vert {
        let i0 = v_chain_indices[v];
        let i1 = v_chain_indices[v + 1];
        let mut edges: Vec<(&[Pt], bool)> = Vec::new();
        if v == 0 {
            let ant_arc = build_inner_arc(&inner_ring_segs, idx_ant_l, idx_ant_r, k_count);
            edges.extend(ant_arc);
        } else {
            edges.push((v_seams.get(&v).unwrap().as_slice(), false));
        }
        for s in i0..i1 {
            edges.push((segs_r[s].as_slice(), false));
        }
        if v == p.n_vert - 1 {
            // Posterior rim arc is reached via the interior junction.
            edges.push((post_med_r.as_slice(), false));
            let post_arc = build_inner_arc(&inner_ring_segs, idx_post_r, idx_post_l, k_count);
            edges.extend(post_arc);
            edges.push((post_med_l.as_slice(), true));
        } else {
            edges.push((v_seams.get(&(v + 1)).unwrap().as_slice(), true));
        }
        for s in (i0..i1).rev() {
            edges.push((segs_l[s].as_slice(), true));
        }
        let poly = concat_edges(&edges);
        scutes.push(Scute::new("vertebral", v, 0, poly));
    }

    // Assemble right and left costal (pleural) scute polygons.
    for c in 0..p.n_costal {
        let i0 = c_chain_indices[c];
        let i1 = c_chain_indices[c + 1];

        // Right costal
        let mut edges: Vec<(&[Pt], bool)> = Vec::new();
        if c > 0 {
            edges.push((c_seams_r.get(&c).unwrap().as_slice(), true));
        }
        for s in i0..i1 {
            edges.push((segs_r[s].as_slice(), false));
        }
        if c < p.n_costal - 1 {
            edges.push((c_seams_r.get(&(c + 1)).unwrap().as_slice(), false));
            let mut rim_arc_r =
                build_inner_arc(&inner_ring_segs, c_attach_r[c], c_attach_r[c + 1], k_count);
            rim_arc_r.reverse();
            for (pts, rev) in rim_arc_r {
                edges.push((pts, !rev));
            }
        } else {
            // Last costal closes via the junction-to-rim posteromedial seam.
            edges.push((post_med_r.as_slice(), false));
            let mut rim_tail_r =
                build_inner_arc(&inner_ring_segs, c_attach_r[c], idx_post_r, k_count);
            rim_tail_r.reverse();
            for (pts, rev) in rim_tail_r {
                edges.push((pts, !rev));
            }
        }
        let poly = concat_edges(&edges);
        scutes.push(Scute::new("costal", c, 1, poly));

        // Left costal
        let mut edges: Vec<(&[Pt], bool)> = Vec::new();
        if c > 0 {
            edges.push((c_seams_l.get(&c).unwrap().as_slice(), false));
        }
        if c < p.n_costal - 1 {
            let mut rim_arc_l =
                build_inner_arc(&inner_ring_segs, c_attach_l[c + 1], c_attach_l[c], k_count);
            rim_arc_l.reverse();
            for (pts, rev) in rim_arc_l {
                edges.push((pts, !rev));
            }
            edges.push((c_seams_l.get(&(c + 1)).unwrap().as_slice(), true));
        } else {
            let mut rim_tail_l =
                build_inner_arc(&inner_ring_segs, idx_post_l, c_attach_l[c], k_count);
            rim_tail_l.reverse();
            for (pts, rev) in rim_tail_l {
                edges.push((pts, !rev));
            }
            edges.push((post_med_l.as_slice(), true));
        }
        for s in (i0..i1).rev() {
            edges.push((segs_l[s].as_slice(), true));
        }
        let poly = concat_edges(&edges);
        scutes.push(Scute::new("costal", c, -1, poly));
    }

    // Collect all unique seam polylines so each seam is stroked strictly once.
    let mut all_seams: Vec<Vec<Pt>> = Vec::new();
    all_seams.extend(outer_arcs.iter().cloned());
    all_seams.extend(radial_seams.iter().cloned());
    all_seams.extend(inner_ring_segs.iter().cloned());
    all_seams.extend(segs_r.iter().cloned());
    all_seams.extend(segs_l.iter().cloned());
    all_seams.extend(v_seams.values().cloned());
    all_seams.extend(c_seams_r.values().cloned());
    all_seams.extend(c_seams_l.values().cloned());
    all_seams.push(post_med_r.clone());
    all_seams.push(post_med_l.clone());

    let outline_edges: Vec<(&[Pt], bool)> =
        outer_arcs.iter().map(|a| (a.as_slice(), false)).collect();
    let full_outline = concat_edges(&outline_edges);
    let full_bounds = poly_bounds(&full_outline);

    CarapaceGraph {
        scutes,
        all_seams,
        outer_arcs,
        full_outline,
        full_bounds,
        cx,
        cy,
        rx,
        ry,
    }
}
