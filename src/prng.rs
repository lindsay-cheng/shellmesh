//! Deterministic PRNG, Perlin noise, and small formatting helpers.
//!
//! This is a bit-accurate port of `js/prng.js`, `js/names.js`, and the
//! `hslCss` helper from `js/palette.js`.


pub const PERLIN_SIZE: usize = 4095;

pub const GENERA_PFX: [&str; 64] = [
    "Testu", "Chelo", "Chryse", "Grapte", "Cycle", "Malacle", "Astro", "Geo", "Cuo", "Kini",
    "Pyxi", "Emydo", "Triony", "Batagu", "Podoc", "Heose", "Maure", "Glypte", "Terrape", "Deiro",
    "Plastro", "Carapo", "Scuto", "Sulco", "Umbo", "Tholo", "Loric", "Tegmo", "Clype", "Fornic",
    "Tessero", "Stereo", "Pachy", "Platy", "Steno", "Macro", "Micro", "Crypto", "Pleuro", "Pelto",
    "Cherso", "Psammo", "Potamo", "Limno", "Thalasso", "Nesio", "Oreo", "Xantho", "Melano",
    "Ochro", "Pyrrho", "Porphyro", "Chalco", "Sidero", "Litho", "Ammo", "Dendro", "Gyro",
    "Spheno", "Rhomb", "Hexa", "Prisma", "Stigma", "Calli",
];

pub const GENERA_SFX: [&str; 28] = [
    "do", "nia", "mys", "chelys", "ura", "xys", "pus", "derm", "testa", "pelta", "scuta", "baka",
    "bates", "caris", "corys", "concha", "cupa", "doma", "gale", "lophus", "notus", "ostracon",
    "parma", "plax", "pyrgos", "sark", "stegos", "theca",
];

pub const SPECIES: [&str; 96] = [
    "radiata", "ocellata", "picta", "serrata", "ornata", "marmorata", "concentrica",
    "flavomarginata", "guttata", "carinata", "reticulata", "elegans", "areolata", "pulchra",
    "insculpta", "spinosa", "imbricata", "tectifera", "rubriventris", "scripta", "polyzonia",
    "annulata", "stellata", "vermiculata", "pyramidalis", "fornicata", "umbonata", "sulcata",
    "tesselata", "hexagona", "clypeata", "loricata", "cristata", "denticulata", "crenulata",
    "lobata", "flabellata", "undulata", "sinuata", "flexuosa", "labyrinthica", "calligrapha",
    "hieroglyphica", "meandrina", "rivulosa", "venosa", "ramosa", "furcata", "lunata", "zonata",
    "fasciata", "vittata", "lineata", "striata", "flammigera", "solaris", "coronata", "aurata",
    "ochracea", "xanthica", "succinea", "ferruginea", "cuprea", "castanea", "brunnea", "nigricans",
    "melanaria", "atrata", "fuliginosa", "fumosa", "cinerea", "olivacea", "viridula", "prasina",
    "smaragdina", "eburnea", "lactea", "ochroleuca", "chrysopicta", "aurostellata",
    "nigrolineata", "flavopunctata", "rubromarginata", "multipunctata", "miliaris", "lentiginosa",
    "maculosa", "nebulosa", "pardalina", "tigrina", "pavonina", "kaleidoscopica", "prismatica",
    "specularis", "nitida", "rugosa",
];

#[derive(Clone)]
pub struct Rng {
    pub jsr: u32,
    pub perlin: [f32; 4096],
}

impl Rng {
    pub fn new(seed_str: &str) -> Self {
        let mut rng = Self {
            jsr: 0,
            perlin: [0.0; 4096],
        };
        rng.seed_prng(seed_str);
        rng.init_perlin();
        rng
    }

    pub fn seed_prng(&mut self, s: &str) {
        let mut h: u32 = 2_166_136_261;
        // JS `str.charCodeAt(i)` iterates UTF-16 code units.
        for code in s.encode_utf16() {
            h ^= code as u32;
            h = h.wrapping_mul(16_777_619);
        }
        self.jsr = if h != 0 { h } else { 0x1337_BEEF };
        for _ in 0..12 {
            self.rand();
        }
    }

    #[inline]
    pub fn rand(&mut self) -> f64 {
        // Exact JS xorshift32 on a uint32 view of the same bits.
        self.jsr ^= self.jsr << 17;
        self.jsr ^= self.jsr >> 13;
        self.jsr ^= self.jsr << 5;
        (self.jsr as f64) / 4_294_967_296.0
    }

    #[inline]
    pub fn rand_range(&mut self, a: f64, b: f64) -> f64 {
        a + self.rand() * (b - a)
    }

    #[inline]
    pub fn rand_int(&mut self, a: i32, b: i32) -> i32 {
        let lo = a as f64;
        let hi = (b + 1) as f64;
        (lo + self.rand() * (hi - lo)).floor() as i32
    }

    #[inline]
    pub fn choice<'a, T>(&mut self, arr: &'a [T]) -> &'a T {
        let idx = (self.rand() * arr.len() as f64).floor() as usize;
        &arr[idx]
    }

    pub fn init_perlin(&mut self) {
        for i in 0..=PERLIN_SIZE {
            // JS stores into a Float32Array: round every sample to f32.
            self.perlin[i] = self.rand() as f32;
        }
    }

    /// Port of `rawNoise`. The `perlin` table is `Float32Array` in JS, so
    /// values are widened from `f32` exactly as the browser does.
    pub fn raw_noise(&self, x: f64, y: f64, z: f64) -> f64 {
        let x = if x < 0.0 { -x } else { x };
        let y = if y < 0.0 { -y } else { y };
        let z = if z < 0.0 { -z } else { z };

        let mut xi = x.floor() as i32;
        let mut yi = y.floor() as i32;
        let mut zi = z.floor() as i32;
        let mut xf = x - xi as f64;
        let mut yf = y - yi as f64;
        let mut zf = z - zi as f64;

        let mut r = 0.0;
        let mut ampl = 0.5;

        for _ in 0..4 {
            let mut of = (xi as u32)
                .wrapping_add((yi as u32) << 4)
                .wrapping_add((zi as u32) << 8);

            let rxf = scaled_cos(xf);
            let ryf = scaled_cos(yf);

            let mut n1 = self.perlin[(of & PERLIN_SIZE as u32) as usize] as f64;
            n1 += rxf
                * (self.perlin[((of + 1) & PERLIN_SIZE as u32) as usize] as f64 - n1);

            let mut n2 = self.perlin[((of + 16) & PERLIN_SIZE as u32) as usize] as f64;
            n2 += rxf
                * (self.perlin[((of + 17) & PERLIN_SIZE as u32) as usize] as f64 - n2);
            n1 += ryf * (n2 - n1);

            of = of.wrapping_add(256);

            n2 = self.perlin[(of & PERLIN_SIZE as u32) as usize] as f64;
            n2 += rxf
                * (self.perlin[((of + 1) & PERLIN_SIZE as u32) as usize] as f64 - n2);

            let mut n3 = self.perlin[((of + 16) & PERLIN_SIZE as u32) as usize] as f64;
            n3 += rxf
                * (self.perlin[((of + 17) & PERLIN_SIZE as u32) as usize] as f64 - n3);
            n2 += ryf * (n3 - n2);

            n1 += scaled_cos(zf) * (n2 - n1);
            r += n1 * ampl;
            ampl *= 0.5;

            xi = xi.wrapping_shl(1);
            xf *= 2.0;
            yi = yi.wrapping_shl(1);
            yf *= 2.0;
            zi = zi.wrapping_shl(1);
            zf *= 2.0;
            if xf >= 1.0 {
                xi = xi.wrapping_add(1);
                xf -= 1.0;
            }
            if yf >= 1.0 {
                yi = yi.wrapping_add(1);
                yf -= 1.0;
            }
            if zf >= 1.0 {
                zi = zi.wrapping_add(1);
                zf -= 1.0;
            }
        }

        r
    }

    #[inline]
    pub fn snoise(&self, x: f64, y: f64, z: f64) -> f64 {
        (self.raw_noise(x, y, z) - 0.5) * 2.0
    }

    pub fn generate_binomial(&mut self) -> String {
        // Preserve the three choice() calls in their original order.
        let pfx = *self.choice(&GENERA_PFX);
        let sfx = *self.choice(&GENERA_SFX);
        let species = *self.choice(&SPECIES);
        // Real-genus collision guard: ensure no generated genus is a real extant genus.
        let mut genus = String::with_capacity(pfx.len() + sfx.len());
        genus.push_str(pfx);
        genus.push_str(sfx);
        const REAL_GENERA: [&str; 9] = [
            "Chelonia", "Chrysemys", "Graptemys", "Cyclemys", "Heosemys", "Mauremys",
            "Glyptemys", "Testudo", "Emydura",
        ];
        if REAL_GENERA.contains(&genus.as_str()) {
            genus.clear();
            genus.push_str(pfx);
            genus.push_str("pelta");
        }
        let mut out = String::with_capacity(genus.len() + 1 + species.len());
        out.push_str(&genus);
        out.push(' ');
        out.push_str(species);
        out
    }
}

#[inline]
fn scaled_cos(t: f64) -> f64 {
    0.5 * (1.0 - (t * core::f64::consts::PI).cos())
}

#[inline]
pub fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

#[inline]
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

pub fn hsl_css(h: f64, s: f64, l: f64) -> String {
    let h = ((h % 360.0) + 360.0) % 360.0;
    let s = clamp(s, 0.0, 100.0);
    let l = clamp(l, 4.0, 95.0);
    let mut out = String::with_capacity(32);
    out.push_str("hsl(");
    push_f1(&mut out, h);
    out.push(',');
    push_f1(&mut out, s);
    out.push_str("%,");
    push_f1(&mut out, l);
    out.push_str("%)");
    out
}

#[inline]
pub fn push_u64(buf: &mut String, mut n: u64) {
    if n == 0 {
        buf.push('0');
        return;
    }
    let mut digits = [0u8; 20];
    let mut i = 20;
    while n > 0 {
        i -= 1;
        digits[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    // SAFETY: digits[i..20] are ASCII '0'..'9'
    let s = unsafe { core::str::from_utf8_unchecked(&digits[i..20]) };
    buf.push_str(s);
}

/// Exact, zero-allocation port of JS `v.toFixed(1)` using Fast2Sum on `10*x = 2*(4*x + x)`.
/// Since multiplying by 4 and 2 are exact exponent shifts in IEEE-754,
/// `s = 2*(4*x + x)` and `err = 2*(x - (s/2 - 4*x))` give the exact 106-bit
/// product `10 * x = s + err` in 5 float ops (no string formatting needed).
#[inline]
pub fn push_f1(buf: &mut String, v: f64) {
    if !v.is_finite() {
        buf.push_str("0.0");
        return;
    }
    let neg = v < 0.0;
    let x = v.abs();
    let x4 = x * 4.0;
    let half_s = x4 + x;
    let err = (x - (half_s - x4)) * 2.0;
    let s = half_s * 2.0;
    let fl = s.floor();
    let diff = s - fl;
    let round_up = if diff > 0.5 {
        true
    } else if diff < 0.5 {
        false
    } else {
        err >= 0.0
    };
    let scaled = (fl as u64) + u64::from(round_up);
    if neg && scaled > 0 {
        buf.push('-');
    }
    push_u64(buf, scaled / 10);
    buf.push('.');
    buf.push((b'0' + (scaled % 10) as u8) as char);
}

/// Fast 3-decimal opacity writer (`v.toFixed(3)`) with zero `core::fmt` overhead.
#[inline]
pub fn push_f3(buf: &mut String, v: f64) {
    if !v.is_finite() || v <= 0.0 {
        buf.push_str("0.000");
        return;
    }
    let scaled = (v * 1000.0 + 0.5 + 1e-12).floor() as u64;
    push_u64(buf, scaled / 1000);
    buf.push('.');
    let rem = (scaled % 1000) as u32;
    buf.push((b'0' + ((rem / 100) % 10) as u8) as char);
    buf.push((b'0' + ((rem / 10) % 10) as u8) as char);
    buf.push((b'0' + (rem % 10) as u8) as char);
}
