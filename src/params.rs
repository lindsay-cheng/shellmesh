//! Palette table and parameter gene dictionary.
//!
//! Exact port of `js/palette.js` and `js/params.js`. The order of every
//! `Rng::rand()` call is preserved.

use crate::prng::{clamp, Rng};

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub name: &'static str,
    pub base: [f64; 3],
    pub sec: [f64; 3],
    pub dark: [f64; 3],
    pub acc1: [f64; 3],
    pub acc2: [f64; 3],
    pub rim: [f64; 3],
}

pub const PALETTES: [Palette; 8] = [
    Palette {
        name: "Astrochelys (Radiated Amber & Umber)",
        base: [34.0, 50.0, 21.0],
        sec: [38.0, 64.0, 33.0],
        dark: [24.0, 58.0, 11.0],
        acc1: [43.0, 88.0, 79.0],
        acc2: [32.0, 76.0, 58.0],
        rim: [40.0, 74.0, 68.0],
    },
    Palette {
        name: "Chrysemys (Pond Olive, Vermilion & Ochre)",
        base: [82.0, 46.0, 27.0],
        sec: [68.0, 44.0, 37.0],
        dark: [95.0, 55.0, 12.0],
        acc1: [14.0, 88.0, 74.0],
        acc2: [46.0, 90.0, 78.0],
        rim: [16.0, 86.0, 68.0],
    },
    Palette {
        name: "Eretmochelys (Hawksbill Tortoiseshell)",
        base: [22.0, 70.0, 27.0],
        sec: [34.0, 80.0, 52.0],
        dark: [16.0, 70.0, 11.0],
        acc1: [42.0, 90.0, 77.0],
        acc2: [14.0, 78.0, 44.0],
        rim: [38.0, 82.0, 65.0],
    },
    Palette {
        name: "Malaclemys (Diamondback Sage & Charcoal)",
        base: [45.0, 32.0, 58.0],
        sec: [110.0, 24.0, 46.0],
        dark: [210.0, 32.0, 12.0],
        acc1: [42.0, 78.0, 78.0],
        acc2: [38.0, 62.0, 82.0],
        rim: [42.0, 52.0, 72.0],
    },
    Palette {
        name: "Terrapene (Ornate Box Mahogany & Gold)",
        base: [26.0, 60.0, 25.0],
        sec: [18.0, 56.0, 35.0],
        dark: [20.0, 68.0, 11.0],
        acc1: [45.0, 92.0, 80.0],
        acc2: [28.0, 84.0, 64.0],
        rim: [44.0, 86.0, 70.0],
    },
    Palette {
        name: "Clemmys (Spotted Slate & Cadmium)",
        base: [220.0, 26.0, 21.0],
        sec: [35.0, 36.0, 27.0],
        dark: [230.0, 38.0, 10.0],
        acc1: [46.0, 92.0, 84.0],
        acc2: [22.0, 76.0, 58.0],
        rim: [28.0, 80.0, 64.0],
    },
    Palette {
        name: "Graptemys (Sawback Moss & Sunburst)",
        base: [74.0, 40.0, 33.0],
        sec: [54.0, 44.0, 45.0],
        dark: [84.0, 50.0, 13.0],
        acc1: [44.0, 90.0, 80.0],
        acc2: [30.0, 86.0, 66.0],
        rim: [42.0, 86.0, 72.0],
    },
    Palette {
        name: "Pyxis (Spider Tortoise Ivory & Sepia)",
        base: [32.0, 56.0, 23.0],
        sec: [38.0, 46.0, 41.0],
        dark: [25.0, 62.0, 11.0],
        acc1: [42.0, 68.0, 84.0],
        acc2: [36.0, 76.0, 64.0],
        rim: [40.0, 64.0, 78.0],
    },
];

#[derive(Clone, Debug)]
pub struct Params {
    pub palette_name: &'static str,
    pub col_base: [f64; 3],
    pub col_sec: [f64; 3],
    pub col_dark: [f64; 3],
    pub col_acc1: [f64; 3],
    pub col_acc2: [f64; 3],
    pub col_rim: [f64; 3],
    pub pattern: &'static str,
    pub interior: &'static str,
    pub aspect: f64,
    pub pyriform: f64,
    pub boxiness: f64,
    pub waist: f64,
    pub hip_flare: f64,
    pub nuchal_notch: f64,
    pub caudal_notch: f64,
    pub serration: f64,
    pub serr_sharp: f64,
    pub rim_morph: &'static str,
    pub rim_width: f64,
    pub n_vert: usize,
    pub n_costal: usize,
    pub n_marg: usize,
    pub fused_supracaudal: bool,
    pub vert_width: f64,
    pub hex_peak: f64,
    pub disc_grid: &'static str,
    pub spine_gamma: f64,
    pub zigzag: f64,
    pub asym: f64,
    pub keel_mode: i32,
    pub inverted: bool,
    pub dome_k: f64,
    pub pyramiding: f64,
    pub ring_count: usize,
    pub ring_dropout: f64,
    pub areola_scale: f64,
    pub areola_shift_y: f64,
    pub areola_shift_x: f64,
    pub striae_density: f64,
    pub hatch_density: f64,
    pub n_rays: usize,
    /// Empty string when JS would leave `starMode` undefined.
    pub star_mode: &'static str,
}

const PATTERN_TYPES: [&str; 6] = [
    "radiating",
    "ocellated",
    "marbled",
    "reticulated",
    "spotted",
    "concentrica",
];

const INTERIOR_TYPES: [&str; 9] = [
    "concentric",
    "concentric",
    "concentric",
    "starburst",
    "hieroglyph",
    "boss",
    "smooth",
    "net",
    "punctate",
];

const STAR_MODES: [&str; 3] = ["bold", "fine", "snow"];

#[inline]
fn shift_col(rng: &mut Rng, c: [f64; 3], hue_shift: f64) -> [f64; 3] {
    [
        c[0] + hue_shift,
        clamp(c[1] + rng.rand_range(-8.0, 8.0), 10.0, 95.0),
        clamp(c[2] + rng.rand_range(-5.0, 5.0), 10.0, 88.0),
    ]
}

pub fn gen_params(rng: &mut Rng) -> Params {
    // Everything below intentionally follows the evaluation order of the JS
    // object literal in params.js. Do not reorder: the RNG is stateful.
    let pal = *rng.choice(&PALETTES);
    let hue_shift = rng.rand_range(-14.0, 14.0);

    let n_vert = *rng.choice(&[5usize, 5, 5, 5, 5, 6, 6, 4]);
    let n_costal = *rng.choice(&[4usize, 4, 4, 4, 5, 5, 3]);
    let n_marg = *rng.choice(&[11usize, 11, 12, 12, 12, 13]);

    let col_base = shift_col(rng, pal.base, hue_shift);
    let col_sec = shift_col(rng, pal.sec, hue_shift);
    let col_dark = shift_col(rng, pal.dark, hue_shift);
    let col_acc1 = shift_col(rng, pal.acc1, hue_shift);
    let col_acc2 = shift_col(rng, pal.acc2, hue_shift);
    let col_rim = shift_col(rng, pal.rim, hue_shift);

    let pattern = *rng.choice(&PATTERN_TYPES);
    let interior = *rng.choice(&INTERIOR_TYPES);

    let aspect = rng.rand_range(0.58, 1.02);
    let pyriform = rng.rand_range(-0.16, 0.20);
    let boxiness = rng.rand_range(1.95, 3.10);
    let waist = rng.rand_range(-0.07, 0.04);
    let hip_flare = rng.rand_range(-0.05, 0.08);
    let nuchal_notch = rng.rand_range(-0.045, -0.005);
    let caudal_notch = rng.rand_range(0.0, 0.04);

    let serration = if rng.rand() < 0.48 {
        rng.rand_range(0.018, 0.058)
    } else {
        rng.rand_range(0.002, 0.012)
    };
    let serr_sharp = rng.rand_range(0.4, 1.5);

    let rim_morph = *rng.choice(&[
        "collared",
        "collared",
        "collared",
        "collared",
        "narrow_rim",
        "flared_skirt",
        "scalloped",
        "sawback",
    ]);

    let fused_supracaudal = rng.rand() < 0.35;

    let r = rng.rand();
    let vert_width = if r < 0.25 {
        rng.rand_range(0.16, 0.25)
    } else if r < 0.75 {
        rng.rand_range(0.30, 0.44)
    } else {
        rng.rand_range(0.48, 0.64)
    };

    let hex_peak = rng.rand_range(0.08, 0.19);

    let disc_grid = *rng.choice(&[
        "staggered_hex",
        "staggered_hex",
        "staggered_hex",
        "quadrate_brick",
        "alternating_dovetail",
    ]);

    let spine_gamma = rng.rand_range(0.72, 1.45);

    let zigzag = if rng.rand() < 0.20 {
        rng.rand_range(0.010, 0.026)
    } else {
        0.0
    };
    let asym = rng.rand_range(0.006, 0.024);
    let keel_mode = *rng.choice(&[0i32, 1, 1, 1]);
    let inverted = rng.rand() < 0.35;
    let dome_k = rng.rand_range(0.24, 0.52);
    let pyramiding = rng.rand_range(0.06, 0.20);
    let ring_count = rng.rand_int(7, 14) as usize;
    let ring_dropout = rng.rand_range(0.18, 0.42);
    let areola_scale = rng.rand_range(0.18, 0.33);
    let areola_shift_y = rng.rand_range(0.12, 0.28);
    let areola_shift_x = rng.rand_range(0.10, 0.24);
    let striae_density = rng.rand_range(0.35, 0.95);
    let hatch_density = rng.rand_range(0.4, 0.9);
    let n_rays = rng.rand_int(5, 11) as usize;

    let mut p = Params {
        palette_name: pal.name,
        col_base,
        col_sec,
        col_dark,
        col_acc1,
        col_acc2,
        col_rim,
        pattern,
        interior,
        aspect,
        pyriform,
        boxiness,
        waist,
        hip_flare,
        nuchal_notch,
        caudal_notch,
        serration,
        serr_sharp,
        rim_morph,
        rim_width: 0.0,
        n_vert,
        n_costal,
        n_marg,
        fused_supracaudal,
        vert_width,
        hex_peak,
        disc_grid,
        spine_gamma,
        zigzag,
        asym,
        keel_mode,
        inverted,
        dome_k,
        pyramiding,
        ring_count,
        ring_dropout,
        areola_scale,
        areola_shift_y,
        areola_shift_x,
        striae_density,
        hatch_density,
        n_rays,
        star_mode: "",
    };

    // Interior architecture overrides: ring/striae presence follows the
    // morphotype.
    if p.interior == "smooth" {
        p.ring_count = 0;
        p.striae_density = 0.0;
    } else if p.interior == "hieroglyph" {
        p.ring_count = rng.rand_int(0, 3) as usize;
        p.ring_dropout = rng.rand_range(0.5, 0.7);
    } else if p.interior == "punctate" {
        p.ring_count = rng.rand_int(0, 4) as usize;
        p.ring_dropout = rng.rand_range(0.5, 0.75);
    } else if p.interior == "boss" {
        p.ring_count = rng.rand_int(8, 14) as usize;
        p.areola_shift_y = rng.rand_range(0.28, 0.42);
        p.pyramiding = rng.rand_range(0.18, 0.30);
    } else if p.interior == "net" {
        p.ring_count = rng.rand_int(2, 6) as usize;
    } else if p.interior == "starburst" {
        p.ring_count = rng.rand_int(6, 12) as usize;
    }

    // Well-conditioned archetype envelopes, co-adapted per interior.
    if p.interior == "starburst" {
        p.aspect = rng.rand_range(0.78, 0.90);
        p.rim_morph = *rng.choice(&["scalloped", "scalloped", "flared_skirt"]);
        p.n_rays = rng.rand_int(6, 12) as usize;
        p.disc_grid = "staggered_hex";
        p.inverted = rng.rand() < 0.6;
        p.star_mode = *rng.choice(&STAR_MODES);
        p.areola_shift_y = p.areola_shift_y.min(0.24);
    } else if p.interior == "hieroglyph" {
        p.aspect = rng.rand_range(0.72, 0.86);
        p.hip_flare = rng.rand_range(0.03, 0.07);
        p.rim_morph = *rng.choice(&["collared", "collared", "flared_skirt"]);
        p.inverted = rng.rand() < 0.5;
    } else if p.interior == "smooth" {
        p.aspect = rng.rand_range(0.65, 0.82);
        p.rim_morph = *rng.choice(&["narrow_rim", "narrow_rim", "collared"]);
        p.disc_grid = "quadrate_brick";
        p.pyramiding = 0.0;
    } else if p.interior == "net" {
        p.rim_morph = *rng.choice(&["sawback", "sawback", "collared"]);
        p.keel_mode = 1;
        p.ring_count = rng.rand_int(0, 2) as usize;
    } else if p.interior == "punctate" {
        p.aspect = rng.rand_range(0.80, 0.98);
    } else if p.interior == "boss" {
        p.aspect = rng.rand_range(0.80, 0.92);
        p.rim_morph = *rng.choice(&["scalloped", "flared_skirt", "collared"]);
    } else {
        // concentric: tortoiseshell envelope
        p.aspect = rng.rand_range(0.66, 0.82);
        p.waist = rng.rand_range(-0.05, 0.0);
        p.rim_morph = *rng.choice(&["scalloped", "flared_skirt", "collared", "collared"]);
    }

    // Radiating shells outside the starburst envelope still get the dual web.
    if p.pattern == "radiating" && p.interior != "starburst" {
        p.star_mode = *rng.choice(&["bold", "bold", "fine", "snow"]);
    }

    // Watermelon fix: smooth banded shells never pair with green pond-turtle
    // palettes. Repick a warm palette using the original hue shift.
    if p.interior == "smooth"
        && (pal.name.starts_with("Chrysemys") || pal.name.starts_with("Graptemys"))
    {
        let warm = [
            PALETTES[0], // Astrochelys
            PALETTES[2], // Eretmochelys
            PALETTES[4], // Terrapene
            PALETTES[7], // Pyxis
        ];
        let idx = ((rng.rand() * warm.len() as f64).floor() as usize) % warm.len();
        let pal2 = warm[idx];
        p.palette_name = pal2.name;
        p.col_base = shift_col(rng, pal2.base, hue_shift);
        p.col_sec = shift_col(rng, pal2.sec, hue_shift);
        p.col_dark = shift_col(rng, pal2.dark, hue_shift);
        p.col_acc1 = shift_col(rng, pal2.acc1, hue_shift);
        p.col_acc2 = shift_col(rng, pal2.acc2, hue_shift);
        p.col_rim = shift_col(rng, pal2.rim, hue_shift);
    }

    // Rim collar width follows the morphotype.
    if p.rim_morph == "narrow_rim" {
        p.rim_width = rng.rand_range(0.045, 0.075);
    } else if p.rim_morph == "flared_skirt" {
        p.rim_width = rng.rand_range(0.09, 0.13);
    } else if p.rim_morph == "scalloped" {
        p.rim_width = rng.rand_range(0.09, 0.14);
    } else if p.rim_morph == "sawback" {
        p.rim_width = rng.rand_range(0.10, 0.15);
    } else {
        p.rim_width = rng.rand_range(0.09, 0.14);
    }

    p
}
