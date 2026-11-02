//! Geometry utilities shared by the carapace graph, pigment engine, and ink
//! pass. Consolidated distance/clipping helpers from `js/pigment.js`.

use crate::carapace::{CarapaceGraph, Scute};
use crate::prng::{clamp, lerp, Rng};

pub type Pt = [f64; 2];

#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub w: f64,
    pub h: f64,
}

impl Bounds {
    #[inline]
    pub fn contains(&self, p: Pt) -> bool {
        p[0] >= self.min_x && p[0] <= self.max_x && p[1] >= self.min_y && p[1] <= self.max_y
    }
}

#[inline]
pub fn dist(a: Pt, b: Pt) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

pub fn centroid(poly: &[Pt]) -> Pt {
    let n = poly.len();
    if n == 0 {
        return [0.0, 0.0];
    }
    let mut sx = 0.0;
    let mut sy = 0.0;
    let mut a = 0.0;
    for i in 0..n {
        let p0 = poly[i];
        let p1 = poly[(i + 1) % n];
        let f = p0[0] * p1[1] - p1[0] * p0[1];
        sx += (p0[0] + p1[0]) * f;
        sy += (p0[1] + p1[1]) * f;
        a += f;
    }
    if a.abs() < 1e-5 {
        sx = 0.0;
        sy = 0.0;
        for p in poly {
            sx += p[0];
            sy += p[1];
        }
        return [sx / n as f64, sy / n as f64];
    }
    a *= 3.0;
    [sx / a, sy / a]
}

pub fn point_in_poly(pt: Pt, poly: &[Pt]) -> bool {
    let x = pt[0];
    let y = pt[1];
    let mut inside = false;
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let xi = poly[i][0];
        let yi = poly[i][1];
        let xj = poly[j][0];
        let yj = poly[j][1];
        if ((yi > y) != (yj > y))
            && (x < (xj - xi) * (y - yi) / (yj - yi + 1e-9) + xi)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[inline]
pub fn point_in_scute(pt: Pt, sc: &Scute) -> bool {
    sc.bounds.contains(pt) && point_in_poly(pt, &sc.poly)
}

pub fn poly_bounds(poly: &[Pt]) -> Bounds {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for p in poly {
        if p[0] < min_x {
            min_x = p[0];
        }
        if p[1] < min_y {
            min_y = p[1];
        }
        if p[0] > max_x {
            max_x = p[0];
        }
        if p[1] > max_y {
            max_y = p[1];
        }
    }
    Bounds {
        min_x,
        min_y,
        max_x,
        max_y,
        w: max_x - min_x,
        h: max_y - min_y,
    }
}

/// Shortest distance from a point to a polygon boundary.
pub fn poly_edge_dist(pt: Pt, poly: &[Pt]) -> f64 {
    let mut best = f64::INFINITY;
    let n = poly.len();
    if n == 0 {
        return best;
    }
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        let ex = b[0] - a[0];
        let ey = b[1] - a[1];
        let l2 = ex * ex + ey * ey;
        let mut t = if l2 > 1e-12 {
            ((pt[0] - a[0]) * ex + (pt[1] - a[1]) * ey) / l2
        } else {
            0.0
        };
        t = clamp(t, 0.0, 1.0);
        let d = (pt[0] - (a[0] + ex * t)).hypot(pt[1] - (a[1] + ey * t));
        if d < best {
            best = d;
        }
    }
    best
}

/// Pull a point that escaped a scute back inside along the segment to anchor.
pub fn pull_inside_poly(pt: Pt, anchor: Pt, poly: &[Pt]) -> Pt {
    if point_in_poly(pt, poly) {
        return pt;
    }
    let mut lo = 0.0;
    let mut hi = 1.0;
    let mut out = [anchor[0], anchor[1]];
    for _ in 0..8 {
        let t = 0.5 * (lo + hi);
        let q = [lerp(anchor[0], pt[0], t), lerp(anchor[1], pt[1], t)];
        if point_in_poly(q, poly) {
            lo = t;
            out = q;
        } else {
            hi = t;
        }
    }
    out
}

pub fn make_perturbed_edge(
    rng: &Rng,
    p_a: Pt,
    p_b: Pt,
    steps: usize,
    bow: f64,
    noise_amp: f64,
    seed_off: f64,
) -> Vec<Pt> {
    let dx = p_b[0] - p_a[0];
    let dy = p_b[1] - p_a[1];
    let mut len = dx.hypot(dy);
    if len == 0.0 || len.is_nan() {
        len = 1.0;
    }
    let nx = -dy / len;
    let ny = dx / len;
    let mut pts = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let env = (core::f64::consts::PI * t).sin();
        let bx = lerp(p_a[0], p_b[0], t) + nx * bow * env * len;
        let by = lerp(p_a[1], p_b[1], t) + ny * bow * env * len;
        let n1 = rng.snoise(bx * 0.022 + seed_off, by * 0.022, 1.3) * noise_amp * env;
        let n2 = rng.snoise(bx * 0.022 + seed_off, by * 0.022, 5.7) * noise_amp * env * 0.4;
        pts.push([
            bx + nx * n1 + (dx / len) * n2,
            by + ny * n1 + (dy / len) * n2,
        ]);
    }
    if !pts.is_empty() {
        pts[0] = p_a;
        let last = pts.len() - 1;
        pts[last] = p_b;
    }
    pts
}

/// Concatenate edges into a ring. Reversed edges are walked backwards and the
/// final point of every edge is omitted so shared endpoints appear once.
pub fn concat_edges(edges: &[(&[Pt], bool)]) -> Vec<Pt> {
    let mut poly = Vec::new();
    for &(pts, rev) in edges {
        if pts.is_empty() {
            continue;
        }
        if rev {
            for i in (1..pts.len()).rev() {
                poly.push(pts[i]);
            }
        } else {
            for i in 0..pts.len() - 1 {
                poly.push(pts[i]);
            }
        }
    }
    poly
}

/// Corner relaxation for growth rings and areolae.
pub fn relax_polygon(poly: &[Pt], blend: f64, passes: usize) -> Vec<Pt> {
    if poly.len() < 3 {
        return poly.to_vec();
    }
    let c = centroid(poly);
    let mut rms0 = 0.0;
    for p in poly {
        let dx = p[0] - c[0];
        let dy = p[1] - c[1];
        rms0 += dx * dx + dy * dy;
    }
    rms0 = (rms0 / poly.len() as f64).sqrt();
    let amt = 0.35 * clamp(blend, 0.0, 1.0);
    if amt <= 0.0 || passes == 0 || rms0 < 1e-9 {
        return poly.to_vec();
    }

    let mut pts = poly.to_vec();
    for _ in 0..passes {
        let n = pts.len();
        let mut next = vec![[0.0, 0.0]; n];
        for i in 0..n {
            let prev = pts[(i + n - 1) % n];
            let cur = pts[i];
            let nxt = pts[(i + 1) % n];
            let mx = 0.5 * (prev[0] + nxt[0]);
            let my = 0.5 * (prev[1] + nxt[1]);
            next[i] = [
                cur[0] + (mx - cur[0]) * amt,
                cur[1] + (my - cur[1]) * amt,
            ];
        }
        pts = next;
    }

    let cr = centroid(&pts);
    let mut rms1 = 0.0;
    for p in &pts {
        let dx = p[0] - cr[0];
        let dy = p[1] - cr[1];
        rms1 += dx * dx + dy * dy;
    }
    rms1 = (rms1 / pts.len() as f64).sqrt();
    let s = if rms1 > 1e-9 { rms0 / rms1 } else { 1.0 };
    pts.iter()
        .map(|p| [c[0] + (p[0] - cr[0]) * s, c[1] + (p[1] - cr[1]) * s])
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub struct Pyramid {
    pub fx: f64,
    pub fy: f64,
    pub rad: f64,
    pub rad_sq: f64,
    pub pk: f64,
}

impl Pyramid {
    pub fn new(fx: f64, fy: f64, rad: f64, pk: f64) -> Self {
        Self {
            fx,
            fy,
            rad,
            rad_sq: rad * rad,
            pk,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Dome {
    pub cx: f64,
    pub cy: f64,
    pub rx: f64,
    pub ry: f64,
    pub k: f64,
    pub pyramids: Vec<Pyramid>,
}

impl Dome {
    pub fn new(cx: f64, cy: f64, rx: f64, ry: f64, k: f64, pyramids: Vec<Pyramid>) -> Self {
        Self {
            cx,
            cy,
            rx,
            ry,
            k,
            pyramids,
        }
    }

    /// 2.5D dome foreshortening with optional per-scute cone bumps.
    pub fn project(&self, p: Pt) -> Pt {
        let k = self.k;
        if k == 0.0 && self.pyramids.is_empty() {
            return p;
        }

        let u = (p[0] - self.cx) / self.rx;
        let v = (p[1] - self.cy) / self.ry;
        let r = (u * u + v * v).sqrt().min(1.0);

        let mut qx = p[0];
        let mut qy = p[1];

        if k != 0.0 {
            if r <= 0.0 {
                qx = self.cx;
                qy = self.cy;
            } else {
                let rp = r * (1.0 + k * (1.0 - r * r).max(0.0).sqrt()) / (1.0 + k);
                let s = rp / r;
                let sx = s * (1.0 - 0.08 * k * u * u);
                qx = self.cx + (p[0] - self.cx) * sx;
                qy = self.cy + (p[1] - self.cy) * s;
            }
        }

        for py in &self.pyramids {
            let rad = if py.rad > 1e-6 { py.rad } else { 1e-6 };
            let dx = p[0] - py.fx;
            let dy = p[1] - py.fy;
            // Bounding-box rejection: 39/40 pyramids bail out here with two
            // subtractions and two comparisons, no sqrt.
            if dx.abs() >= rad || dy.abs() >= rad {
                continue;
            }
            let d2 = dx * dx + dy * dy;
            let rad_sq = if py.rad_sq > 0.0 {
                py.rad_sq
            } else {
                rad * rad
            };
            if d2 >= rad_sq {
                continue;
            }
            let d = d2.sqrt() / rad;
            let pk = py.pk;
            if pk > 0.0 && d > 0.0 && d < 1.0 {
                let bow = 1.0 + pk * d * (1.0 - d) * 0.35;
                qx = py.fx + (qx - py.fx) * bow;
                qy = py.fy + (qy - py.fy) * bow - pk * (1.0 - d) * 3.5;
            }
        }

        [qx, qy]
    }
}

/// Fishdraw-style resample: even arc-length vertices, first/last preserved.
pub fn resample_polyline(pts: &[Pt], step_len: f64) -> Vec<Pt> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let mut cum = Vec::with_capacity(pts.len());
    cum.push(0.0);
    for i in 1..pts.len() {
        let d = dist(pts[i - 1], pts[i]);
        cum.push(cum[i - 1] + d);
    }
    let total = cum[cum.len() - 1];
    if total <= 0.0 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    let mut target = step_len;
    let mut j = 1;
    while target < total {
        while j < cum.len() - 1 && cum[j] < target {
            j += 1;
        }
        let denom = (cum[j] - cum[j - 1]).max(1e-9);
        let t = (target - cum[j - 1]) / denom;
        out.push([
            lerp(pts[j - 1][0], pts[j][0], t),
            lerp(pts[j - 1][1], pts[j][1], t),
        ]);
        target += step_len;
    }
    out.push(pts[pts.len() - 1]);
    out
}

#[derive(Clone, Copy, Debug)]
pub struct DisplaceOpts {
    pub step_len: f64,
    pub drift_amp: f64,
    pub jitter_amp: f64,
    pub seed_off: f64,
    pub pin_ends: bool,
}

impl Default for DisplaceOpts {
    fn default() -> Self {
        Self {
            step_len: 3.5,
            drift_amp: 2.2,
            jitter_amp: 0.7,
            seed_off: 0.0,
            pin_ends: true,
        }
    }
}

/// Fishdraw drift + linedraw index-keyed jitter along the normal.
pub fn displace_polyline(
    rng: &Rng,
    pts: &[Pt],
    stroke_idx: f64,
    opts: &DisplaceOpts,
) -> Vec<Pt> {
    let step_len = if opts.step_len == 0.0 {
        3.5
    } else {
        opts.step_len
    };
    let work = resample_polyline(pts, step_len);
    let n = work.len();
    if n < 3 {
        return work;
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f64 / (n - 1) as f64;
        let a = work[i.saturating_sub(1)];
        let b = work[(i + 1).min(n - 1)];
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let mut l = dx.hypot(dy);
        if l == 0.0 || l.is_nan() {
            l = 1.0;
        }
        let nx = -dy / l;
        let ny = dx / l;
        let drift = opts.drift_amp * rng.snoise(stroke_idx * 1.37, t * 1.6, opts.seed_off);
        let jit = opts.jitter_amp * rng.snoise(stroke_idx * 0.7, i as f64 * 0.25, opts.seed_off + 10.0);
        let mut env = 1.0;
        if opts.pin_ends {
            env = (core::f64::consts::PI * t)
                .sin()
                .max(0.0)
                .powf(0.7);
        }
        let d = (drift + jit) * env;
        out.push([work[i][0] + nx * d, work[i][1] + ny * d]);
    }
    out
}

#[inline]
pub fn push_poly_svg_path(d: &mut String, poly: &[Pt], dome: &Dome) {
    if poly.len() < 3 {
        return;
    }
    d.reserve(poly.len() * 14);
    let q0 = dome.project(poly[0]);
    d.push('M');
    crate::prng::push_f1(d, q0[0]);
    d.push(',');
    crate::prng::push_f1(d, q0[1]);
    for p in &poly[1..] {
        let q = dome.project(*p);
        d.push('L');
        crate::prng::push_f1(d, q[0]);
        d.push(',');
        crate::prng::push_f1(d, q[1]);
    }
    d.push('Z');
}

#[inline]
pub fn push_strokes_compound_path(d: &mut String, strokes: &[Vec<Pt>], dome: &Dome) {
    for pts in strokes {
        if pts.len() < 2 {
            continue;
        }
        d.reserve(pts.len() * 14);
        let q0 = dome.project(pts[0]);
        d.push('M');
        crate::prng::push_f1(d, q0[0]);
        d.push(',');
        crate::prng::push_f1(d, q0[1]);
        for p in &pts[1..] {
            let q = dome.project(*p);
            d.push('L');
            crate::prng::push_f1(d, q[0]);
            d.push(',');
            crate::prng::push_f1(d, q[1]);
        }
    }
}

pub fn poly_to_svg_path(poly: &[Pt], dome: &Dome) -> String {
    let mut d = String::new();
    push_poly_svg_path(&mut d, poly, dome);
    d
}

pub fn strokes_to_compound_path(strokes: &[Vec<Pt>], dome: &Dome) -> String {
    let mut d = String::new();
    push_strokes_compound_path(&mut d, strokes, dome);
    d
}

/// WP5 validator: topology + geometry guardrail for carapace edits.
/// Returns the list of errors (empty means valid).
pub fn validate_carapace_graph(graph: &CarapaceGraph) -> Vec<String> {
    validate_carapace_graph_with_focus(graph, None)
}

pub fn validate_carapace_graph_with_focus(
    graph: &CarapaceGraph,
    focus_fn: Option<&dyn Fn(&Scute) -> Pt>,
) -> Vec<String> {
    let mut errors = Vec::new();

    fn area(poly: &[Pt]) -> f64 {
        let mut s = 0.0;
        let n = poly.len();
        for i in 0..n {
            let a = poly[i];
            let b = poly[(i + 1) % n];
            s += (b[0] - a[0]) * (b[1] + a[1]);
        }
        s / 2.0
    }

    for sc in &graph.scutes {
        let a = area(&sc.poly);
        if a.abs() <= 150.0 {
            errors.push(format!(
                "degenerate {}{}: area {:.1}",
                sc.scute_type, sc.index, a
            ));
        }
        if let Some(f) = focus_fn {
            let focus = f(sc);
            if !point_in_poly(focus, &sc.poly) {
                errors.push(format!("focus outside {}{}", sc.scute_type, sc.index));
            }
        }
    }

    // Partition coverage: 25x25 grid, each interior point in exactly 1 scute.
    let mut holes = 0;
    let mut overlaps = 0;
    let mut tested = 0;
    let bb = graph.full_bounds;
    for gx in 0..25 {
        for gy in 0..25 {
            let x = bb.min_x + (bb.max_x - bb.min_x) * (gx as f64 + 0.5) / 25.0;
            let y = bb.min_y + (bb.max_y - bb.min_y) * (gy as f64 + 0.5) / 25.0;
            if !point_in_poly([x, y], &graph.full_outline) {
                continue;
            }
            tested += 1;
            let mut hits = 0;
            for sc in &graph.scutes {
                if sc.bounds.contains([x, y]) && point_in_poly([x, y], &sc.poly) {
                    hits += 1;
                }
            }
            if hits == 0 {
                holes += 1;
            } else if hits > 1 {
                overlaps += 1;
            }
        }
    }
    if holes > 0 {
        errors.push(format!("coverage holes: {}/{}", holes, tested));
    }
    if overlaps > 0 {
        errors.push(format!("coverage overlaps: {}/{}", overlaps, tested));
    }

    errors
}
