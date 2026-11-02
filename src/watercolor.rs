//! Watercolor wash layers via evolutionary polygon distortion.
//!
//! Technique adapted from
//! `@watercolorizer/watercolorizer` by J. Holmes (ISC).

use crate::geometry::{Pt};
use crate::prng::{clamp, Rng};

struct Gauss<'a> {
    rng: &'a mut Rng,
}

impl<'a> Gauss<'a> {
    #[inline]
    fn sample(&mut self, mu: f64, sigma: f64) -> f64 {
        let u = 1.0 - self.rng.rand();
        let v = self.rng.rand();
        let z = (-2.0 * u.ln()).sqrt() * (2.0 * core::f64::consts::PI * v).cos();
        z * sigma + mu
    }
}

fn winding_of(pts: &[Pt]) -> bool {
    // true = clockwise ("cw" in JS)
    let mut s = 0.0;
    let n = pts.len();
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        s += (b[0] - a[0]) * (b[1] + a[1]);
    }
    s > 0.0
}

/// One displaced midpoint for segment a->b; returns (point, weight).
fn wobble_mid(a: Pt, b: Pt, w0: f64, w1: f64, cw: bool, g: &mut Gauss) -> (Pt, f64) {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let mut len = dx.hypot(dy);
    if len == 0.0 || len.is_nan() {
        len = 1.0;
    }
    let t = clamp(g.sample(0.5, 0.4 / 3.0), 0.001, 0.999);
    let mx = a[0] + dx * t;
    let my = a[1] + dy * t;
    let w = w0 + (w1 - w0) * t;

    let mut nx = -dy / len;
    let mut ny = dx / len;
    if !cw {
        nx = -nx;
        ny = -ny;
    }
    let th = g.sample(0.0, core::f64::consts::PI / 12.0);
    let c = th.cos();
    let s = th.sin();
    let rx = nx * c - ny * s;
    let ry = nx * s + ny * c;
    let mut mag = g.sample(0.0, len / 3.0);
    if mag < 0.0 {
        mag /= 5.0;
    }
    mag *= w;
    ([mx + rx * mag, my + ry * mag], w)
}

/// Wiggle each vertex along its angle bisector.
fn wobble_verts(pts: &[Pt], weights: &[f64], g: &mut Gauss) -> Vec<Pt> {
    let n = pts.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let a = pts[(i + n - 1) % n];
        let b = pts[i];
        let c = pts[(i + 1) % n];
        let dx = (b[0] - a[0]) + (b[0] - c[0]);
        let dy = (b[1] - a[1]) + (b[1] - c[1]);
        let l = dx.hypot(dy);
        if l < 1e-9 {
            out.push(b);
            continue;
        }
        let th = g.sample(0.0, 0.5);
        let co = th.cos();
        let si = th.sin();
        let rx = (dx / l) * co - (dy / l) * si;
        let ry = (dx / l) * si + (dy / l) * co;
        let arc = ((b[0] - a[0]).hypot(b[1] - a[1])
            + (c[0] - b[0]).hypot(c[1] - b[1]))
            / 2.0
            / 3.0;
        let m = g.sample(0.0, arc / 3.0).abs() * weights[i];
        out.push([b[0] + rx * m, b[1] + ry * m]);
    }
    out
}

/// One evolution step: interleave wiggled vertices with displaced midpoints.
fn evolve_poly(pts: &[Pt], weights: &[f64], cw: bool, g: &mut Gauss) -> (Vec<Pt>, Vec<f64>) {
    let n = pts.len();
    let mut np = Vec::with_capacity(n * 2);
    let mut nw = Vec::with_capacity(n * 2);
    for _ in 0..n {
        np.push([0.0, 0.0]);
        np.push([0.0, 0.0]);
        nw.push(0.0);
        nw.push(0.0);
    }
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let (m, w) = wobble_mid(a, b, weights[i], weights[(i + 1) % n], cw, g);
        np[2 * i + 1] = m;
        nw[2 * i + 1] = w;
    }
    let wig = wobble_verts(pts, weights, g);
    for i in 0..n {
        np[2 * i] = wig[i];
        nw[2 * i] = weights[i];
    }
    (np, nw)
}

/// Tiny radial-distance simplification (keeps point counts sane).
fn simplify_poly(pts: &[Pt], tol: f64) -> Vec<Pt> {
    if pts.len() < 4 {
        return pts.to_vec();
    }
    let mut keep = Vec::with_capacity(pts.len());
    keep.push(pts[0]);
    let t2 = tol * tol;
    for p in &pts[1..] {
        let last = keep[keep.len() - 1];
        let dx = p[0] - last[0];
        let dy = p[1] - last[1];
        if dx * dx + dy * dy >= t2 {
            keep.push(*p);
        }
    }
    let first = keep[0];
    let last = keep[keep.len() - 1];
    let dx = first[0] - last[0];
    let dy = first[1] - last[1];
    if dx * dx + dy * dy < t2 {
        keep.pop();
    }
    if keep.len() >= 3 {
        keep
    } else {
        pts.to_vec()
    }
}

/// Generate translucent wash layers from a base polygon.
pub fn watercolor_layers(
    poly: &[Pt],
    rng: &mut Rng,
    pre_evolutions: usize,
    evolutions: usize,
    layers_per_evolution: usize,
    layer_evolutions: usize,
) -> Vec<Vec<Pt>> {
    let mut g = Gauss { rng };
    let cw = winding_of(poly);

    let mut prev_pts = poly.to_vec();
    let mut prev_w = vec![1.0; poly.len()];

    for _ in 0..pre_evolutions {
        let (p, w) = evolve_poly(&prev_pts, &prev_w, cw, &mut g);
        prev_pts = p;
        prev_w = w;
    }

    let mut layers = Vec::new();
    for _ in 0..evolutions {
        for _ in 0..layers_per_evolution {
            // distort(prev), then run() for layerEvolutions more steps.
            let (mut p, mut w) = evolve_poly(&prev_pts, &prev_w, cw, &mut g);
            for _ in 0..layer_evolutions {
                let (p2, w2) = evolve_poly(&p, &w, cw, &mut g);
                p = p2;
                w = w2;
            }
            layers.push(simplify_poly(&p, 1.0));
        }
        let (p, w) = evolve_poly(&prev_pts, &prev_w, cw, &mut g);
        prev_pts = p;
        prev_w = w;
    }
    layers
}
