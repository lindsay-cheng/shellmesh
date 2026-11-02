use std::cell::RefCell;

pub mod carapace;
pub mod geometry;
pub mod ink;
pub mod params;
pub mod pigment;
pub mod plate;
pub mod prng;
pub mod watercolor;

#[derive(Default)]
struct WasmState {
    out_svg: String,
    out_meta: String,
    grid_w: usize,
    grid_h: usize,
}

thread_local! {
    static STATE: RefCell<WasmState> = RefCell::new(WasmState::default());
}

fn read_input(ptr: *const u8, len: usize) -> String {
    if ptr.is_null() || len == 0 {
        return String::new();
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    String::from_utf8_lossy(bytes).into_owned()
}

#[no_mangle]
pub extern "C" fn wasm_alloc(len: usize) -> *mut u8 {
    let mut buf: Vec<u8> = Vec::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    core::mem::forget(buf);
    ptr
}

#[no_mangle]
pub extern "C" fn wasm_dealloc(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }
    unsafe { drop(Vec::from_raw_parts(ptr, 0, len)) };
}

#[no_mangle]
pub extern "C" fn wasm_svg_ptr() -> *const u8 {
    STATE.with(|s| s.borrow().out_svg.as_ptr())
}

#[no_mangle]
pub extern "C" fn wasm_svg_len() -> usize {
    STATE.with(|s| s.borrow().out_svg.len())
}

#[no_mangle]
pub extern "C" fn wasm_meta_ptr() -> *const u8 {
    STATE.with(|s| s.borrow().out_meta.as_ptr())
}

#[no_mangle]
pub extern "C" fn wasm_meta_len() -> usize {
    STATE.with(|s| s.borrow().out_meta.len())
}

#[no_mangle]
pub extern "C" fn wasm_grid_w() -> usize {
    STATE.with(|s| s.borrow().grid_w)
}

#[no_mangle]
pub extern "C" fn wasm_grid_h() -> usize {
    STATE.with(|s| s.borrow().grid_h)
}

#[no_mangle]
pub extern "C" fn wasm_generate_shell(seed_ptr: *const u8, seed_len: usize) -> usize {
    let seed = read_input(seed_ptr, seed_len);
    let out = plate::generate_shell_svg(&seed);
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let len = out.svg.len();
        s.out_svg = out.svg;
        s.out_meta = out.meta;
        s.grid_w = 0;
        s.grid_h = 0;
        len
    })
}

#[no_mangle]
pub extern "C" fn wasm_generate_cell(
    cell_idx: usize,
    seed_ptr: *const u8,
    seed_len: usize,
) -> usize {
    let seed = read_input(seed_ptr, seed_len);
    let mut svg = String::with_capacity(1_600_000);
    let _ = plate::write_shell_body(&mut svg, &seed, Some(cell_idx));
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let len = svg.len();
        s.out_svg = svg;
        s.out_meta.clear();
        s.grid_w = 0;
        s.grid_h = 0;
        len
    })
}

#[no_mangle]
pub extern "C" fn wasm_generate_grid(n: usize, seeds_ptr: *const u8, seeds_len: usize) -> usize {
    let text = read_input(seeds_ptr, seeds_len);
    let seeds: Vec<String> = if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n').map(str::to_owned).collect()
    };
    let (svg, w, h) = plate::generate_grid_svg(n, &seeds);
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let len = svg.len();
        s.out_svg = svg;
        s.out_meta = String::new();
        s.grid_w = w;
        s.grid_h = h;
        len
    })
}

#[no_mangle]
pub extern "C" fn wasm_random_seeds(
    count: usize,
    entropy_ptr: *const u8,
    entropy_len: usize,
) -> usize {
    let entropy = read_input(entropy_ptr, entropy_len);
    let mut rng = prng::Rng {
        jsr: 0x5EED,
        perlin: [0.0; 4096],
    };
    rng.seed_prng(&entropy);
    let names: Vec<String> = (0..count).map(|_| rng.generate_binomial()).collect();
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.out_meta = names.join("\n");
        s.out_meta.len()
    })
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;
    use super::carapace::build_carapace_graph;
    use super::geometry::validate_carapace_graph;
    use super::params::gen_params;
    use super::plate::generate_grid_svg;
    use super::plate::generate_shell_svg;
    use super::prng::{hsl_css, push_f1, Rng, GENERA_PFX, GENERA_SFX, SPECIES};

    #[test]
    fn prng_is_deterministic() {
        let mut a = Rng::new("st4-7");
        let mut b = Rng::new("st4-7");
        for _ in 0..1000 {
            assert_eq!(a.rand().to_bits(), b.rand().to_bits());
        }
        let mut c = Rng::new("st4-8");
        assert_ne!(a.rand().to_bits(), c.rand().to_bits());
    }

    #[test]
    fn push_f1_matches_js_to_fixed() {
        let cases: &[(f64, &str)] = &[
            (0.0, "0.0"),
            (-0.0, "0.0"),
            (0.25, "0.3"),
            (-0.25, "-0.3"),
            (0.15, "0.1"),
            (-0.15, "-0.1"),
            (0.05, "0.1"),
            (-0.05, "-0.1"),
            (1.005, "1.0"),
            (123.456, "123.5"),
            (-123.456, "-123.5"),
            (1e-9, "0.0"),
            (999.95, "1000.0"),
            (2.675, "2.7"),
            (0.35, "0.3"),
            (-0.35, "-0.3"),
            (10.05, "10.1"),
            (1.25, "1.3"),
            (1.35, "1.4"),
            (9.95, "9.9"),
            (-9.95, "-9.9"),
            (0.049999999999999996, "0.0"),
            (0.05000000000000001, "0.1"),
        ];
        for &(v, want) in cases {
            let mut s = String::new();
            push_f1(&mut s, v);
            assert_eq!(s, want, "push_f1({v:?})");
        }
    }

    #[test]
    fn hsl_css_matches_js() {
        assert_eq!(hsl_css(370.0, 105.0, 3.0), "hsl(10.0,100.0%,4.0%)");
        assert_eq!(hsl_css(-10.0, 50.0, 50.0), "hsl(350.0,50.0%,50.0%)");
        assert_eq!(hsl_css(720.55, -5.0, 100.0), "hsl(0.5,0.0%,95.0%)");
    }

    #[test]
    fn reference_seed_st4_7() {
        let mut rng = Rng::new("st4-7");
        let p = gen_params(&mut rng);
        assert_eq!(p.palette_name, "Eretmochelys (Hawksbill Tortoiseshell)");
        assert_eq!(p.pattern, "radiating");
        assert_eq!(p.interior, "concentric");
        assert_eq!(p.star_mode, "snow");
        assert_eq!((p.n_vert, p.n_costal, p.n_marg), (5, 4, 11));
        assert!(!p.fused_supracaudal);
        assert_eq!(p.ring_count, 12);
        assert_eq!(p.n_rays, 9);
        assert_eq!(p.aspect.to_bits(), 0.7062618805840611_f64.to_bits());
        assert_eq!(p.rim_width.to_bits(), 0.09875380729557946_f64.to_bits());

        let g = build_carapace_graph(&mut rng, &p, 380.0, 412.0, 312.0);
        assert_eq!(g.scutes.len(), 36);
        assert_eq!(g.outer_arcs.len(), 23);
        assert_eq!(g.full_outline.len(), 276);
        assert!(validate_carapace_graph(&g).is_empty());
    }

    #[test]
    fn carapace_graphs_validate_across_100_seeds() {
        for i in 0..100 {
            let seed = format!("batch1-{i}");
            let mut rng = Rng::new(&seed);
            let p = gen_params(&mut rng);
            let g = build_carapace_graph(&mut rng, &p, 380.0, 412.0, 312.0);
            let k = 2 * p.n_marg + 1 + usize::from(p.fused_supracaudal);
            assert_eq!(
                g.scutes.len(),
                k + p.n_vert + 2 * p.n_costal,
                "unexpected scute count for {seed}"
            );
            let errors = validate_carapace_graph(&g);
            assert!(
                errors.is_empty(),
                "seed {seed} failed validation: {errors:?}"
            );
        }
    }

    #[test]
    fn grid_layout_and_namespacing() {
        let seeds: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let (svg, w, h) = generate_grid_svg(2, &seeds);
        assert_eq!((w, h), (796, 916));
        assert!(svg.contains("carapace-clip-3"));
        assert!(svg.contains("transform=\"translate(404,464) scale(0.5)\""));

        let t0 = std::time::Instant::now();
        let seeds9: Vec<String> = (0..9).map(|i| format!("bench-{i}")).collect();
        let (g3, _, _) = generate_grid_svg(3, &seeds9);
        let dt = t0.elapsed().as_secs_f64() * 1000.0;
        println!("Native Rust 3x3 grid (9 shells): {dt:.1}ms ({:.1}ms/shell), bytes={}", dt / 9.0, g3.len());
    }

    #[test]
    fn cell_export_matches_generate_grid_svg() {
        let seeds: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let (expected, _, _) = generate_grid_svg(2, &seeds);

        const CELL_W: usize = 760;
        const CELL_H: usize = 880;
        const GAP: usize = 12;
        const BG: &str = "#161514";
        let n = 2usize;
        let sw = CELL_W / 2;
        let sh = CELL_H / 2;
        let w = n * sw + (n + 1) * GAP;
        let h = n * sh + (n + 1) * GAP;

        let mut svg = String::with_capacity(n * n * 1_550_000 + 256);
        let _ = write!(svg, "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\"><rect width=\"{w}\" height=\"{h}\" fill=\"{BG}\"/>");
        for i in 0..(n * n) {
            let x = GAP + (i % n) * (sw + GAP);
            let y = GAP + (i / n) * (sh + GAP);
            let _ = write!(svg, "<g data-seed=\"{}\" transform=\"translate({x},{y}) scale(0.5)\">", seeds[i]);
            let mut body = String::new();
            let _ = crate::plate::write_shell_body(&mut body, &seeds[i], Some(i));
            svg.push_str(&body);
            svg.push_str("</g>\n");
        }
        svg.push_str("</svg>");
        assert_eq!(svg, expected);
    }

    #[test]
    fn watermark_and_long_seed_truncation() {
        let out = generate_shell_svg("Podocpelta Marmorata");
        assert!(out.svg.contains(">shellmesh, podocpelta marmorata</text>"));

        let long_seed = "A".repeat(60);
        let out2 = generate_shell_svg(&long_seed);
        assert!(out2.svg.contains(">shellmesh, aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa...</text>"));
    }

    #[test]
    fn binomial_generator_produces_valid_fictional_names() {
        const REAL_GENERA: [&str; 9] = [
            "Chelonia", "Chrysemys", "Graptemys", "Cyclemys", "Heosemys", "Mauremys",
            "Glyptemys", "Testudo", "Emydura",
        ];

        assert_eq!(GENERA_PFX.len(), 64);
        assert_eq!(GENERA_SFX.len(), 28);
        assert_eq!(SPECIES.len(), 96);

        for p in GENERA_PFX {
            assert!(!p.is_empty(), "empty prefix");
            assert_eq!(p, p.trim(), "untrimmed prefix: {p:?}");
            assert!(
                p.chars().all(|c| c.is_ascii_alphabetic()),
                "non-ascii-letter prefix: {p:?}"
            );
        }
        for s in GENERA_SFX {
            assert!(!s.is_empty(), "empty suffix");
            assert_eq!(s, s.trim(), "untrimmed suffix: {s:?}");
            assert!(
                s.chars().all(|c| c.is_ascii_alphabetic()),
                "non-ascii-letter suffix: {s:?}"
            );
        }
        for sp in SPECIES {
            assert!(!sp.is_empty(), "empty species");
            assert_eq!(sp, sp.trim(), "untrimmed species: {sp:?}");
            assert!(
                sp.chars().all(|c| c.is_ascii_alphabetic()),
                "non-ascii-letter species: {sp:?}"
            );
        }

        let mut rng = Rng::new("binomial-test");
        for _ in 0..500 {
            let name = rng.generate_binomial();
            let mut chars = name.chars();
            let first = chars.next().expect("non-empty name");
            assert!(first.is_ascii_uppercase(), "name not starting uppercase: {name}");
            let parts: Vec<&str> = name.split(' ').collect();
            assert_eq!(parts.len(), 2, "expected single space: {name}");
            let genus = parts[0];
            assert!(!REAL_GENERA.contains(&genus), "real genus leaked: {genus}");
        }

        // Existing shell seeds still behave.
        let mut rng = Rng::new("st4-7");
        let _ = rng.generate_binomial();
        let mut rng = Rng::new("Podocpelta marmorata");
        let _ = rng.generate_binomial();
    }
}
