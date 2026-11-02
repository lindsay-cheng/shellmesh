// ui & interaction
const seedInput = document.getElementById("seedInput");
const stage = document.getElementById("stage");
const metaInfo = document.getElementById("metaInfo");
const btnBack = document.getElementById("btnBack");
const btnFwd = document.getElementById("btnFwd");
const gridSize = document.getElementById("gridSize");
const modeSingle = document.getElementById("modeSingle");
const modeGrid = document.getElementById("modeGrid");
const btnGenerate = document.getElementById("btnGenerate");
const btnRandom = document.getElementById("btnRandom");
const btnAnimate = document.getElementById("btnAnimate");
const btnDownload = document.getElementById("btnDownload");
const btnDownloadPng = document.getElementById("btnDownloadPng");
const btnCredits = document.getElementById("btnCredits");
const btnCloseCredits = document.getElementById("btnCloseCredits");
const creditsModal = document.getElementById("creditsModal");
const INIT_SEED = "Testudines_Init_2026";

let wasm = null;
const encoder = new TextEncoder();
const decoder = new TextDecoder();

let currentSeed = "";
let currentSvgString = "";
let metaBase = "";
// view doubles as the mode: exactly one of "single" / "grid" is active
let view = "single";

let renderSeq = 0;

// worker pool state
let wasmModule = null;
let useSyncGrid = false;
const POOL_SIZE = Math.max(2, Math.min((navigator.hardwareConcurrency || 4), 8));
let workers = [];
let idleWorkers = [];
let pendingTasks = [];
let taskCounter = 0;

// one unified timeline:
//   { mode: "single", seed }
//   { mode: "grid", n, seeds, svg?, W?, H? }
// grid entries cache their assembled svg so back/fwd and mode switching are
// instantaneous instead of regenerating n*n shells.
let history = [];
let hIdx = -1;
let lastSingleSeed = null;
let lastGridState = null;

function escapeAttr(s) {
  return String(s).replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function pumpTasks() {
  while (idleWorkers.length > 0 && pendingTasks.length > 0) {
    const worker = idleWorkers.shift();
    const task = pendingTasks.shift();
    worker._currentTask = task;
    worker.postMessage({ type: "cell", taskId: task.id, cellIdx: task.cellIdx, seed: task.seed });
  }
}

async function initWorkers(module) {
  const readyPromises = [];
  for (let i = 0; i < POOL_SIZE; i++) {
    let worker;
    try {
      worker = new Worker("js/worker.js?v=8");    } catch (err) {
      throw err;
    }
    const ready = new Promise((resolve, reject) => {
      const handler = (e) => {
        if (e.data && e.data.type === "ready") {
          worker.removeEventListener("message", handler);
          idleWorkers.push(worker);
          pumpTasks();
          resolve();
        } else if (e.data && e.data.type === "init_error") {
          worker.removeEventListener("message", handler);
          reject(new Error(e.data.message));
        }
      };
      worker.addEventListener("message", handler);
    });
    worker.addEventListener("message", (e) => {
      const d = e.data;
      if (d && (d.type === "cell_done" || d.type === "cell_error")) {
        const task = worker._currentTask;
        worker._currentTask = null;
        if (task) {
          if (d.type === "cell_done") task.resolve(d.inner);
          else task.reject(new Error(d.message));
        }
        idleWorkers.push(worker);
        pumpTasks();
      }
    });
    worker.postMessage({ type: "init", module });
    workers.push(worker);
    readyPromises.push(ready);
  }
  await Promise.all(readyPromises);
}

async function loadWasm() {
  const url = "pkg/shellmesh.wasm?v=8";  let result;
  try {
    result = await WebAssembly.instantiateStreaming(fetch(url), {});
  } catch (err) {
    const res = await fetch(url);
    if (!res.ok) throw new Error(`HTTP ${res.status} for ${url}`);
    const buf = await res.arrayBuffer();
    result = await WebAssembly.instantiate(buf, {});
  }
  wasm = result.instance.exports;
  wasmModule = result.module;
  initWorkers(wasmModule).catch(() => {
    useSyncGrid = true;
    workers = [];
    idleWorkers = [];
  });
  return wasm;
}

function enqueueCellTask(cellIdx, seed) {
  return new Promise((resolve, reject) => {
    const task = { id: ++taskCounter, cellIdx, seed, resolve, reject };
    pendingTasks.push(task);
    pumpTasks();
  });
}

async function buildGridParallel(n, seeds) {
  if (useSyncGrid || workers.length === 0) {
    const built = buildGridSVG(n, seeds);
    return { svg: built.svg, W: built.W, H: built.H };
  }
  const count = n * n;
  const CELL_W = 760, CELL_H = 880, GAP = 12, BG = "#161514";
  const sw = CELL_W / 2, sh = CELL_H / 2;
  const W = n * sw + (n + 1) * GAP;
  const H = n * sh + (n + 1) * GAP;
  const inners = new Array(count);
  const promises = [];
  for (let i = 0; i < count; i++) {
    promises.push(enqueueCellTask(i, seeds[i]).then((inner) => { inners[i] = inner; }));
  }
  await Promise.all(promises);
  let svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}"><rect width="${W}" height="${H}" fill="${BG}"/>`;
  for (let i = 0; i < count; i++) {
    const x = GAP + (i % n) * (sw + GAP);
    const y = GAP + Math.floor(i / n) * (sh + GAP);
    svg += `<g data-seed="${escapeAttr(seeds[i])}" transform="translate(${x},${y}) scale(0.5)">${inners[i]}</g>\n`;
  }
  svg += `</svg>`;
  return { svg, W, H };
}

function writeString(text) {
  const bytes = encoder.encode(text);
  const ptr = wasm.wasm_alloc(bytes.length);
  new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
  return { ptr, len: bytes.length };
}

function freeString(s) {
  wasm.wasm_dealloc(s.ptr, s.len);
}

function readWasmString(ptr, len) {
  return decoder.decode(new Uint8Array(wasm.memory.buffer, ptr, len));
}

function generateShellSVG(seed) {
  const s = writeString(seed);
  wasm.wasm_generate_shell(s.ptr, s.len);
  freeString(s);
  return {
    svg: readWasmString(wasm.wasm_svg_ptr(), wasm.wasm_svg_len()),
    meta: readWasmString(wasm.wasm_meta_ptr(), wasm.wasm_meta_len()),
  };
}

function buildGridSVG(n, seeds) {
  const s = writeString(seeds.join("\n"));
  wasm.wasm_generate_grid(n, s.ptr, s.len);
  freeString(s);
  return {
    svg: readWasmString(wasm.wasm_svg_ptr(), wasm.wasm_svg_len()),
    W: wasm.wasm_grid_w(),
    H: wasm.wasm_grid_h(),
  };
}

function randomSeeds(count, entropy) {
  const s = writeString(entropy !== undefined ? entropy : String(Date.now() + Math.random()));
  wasm.wasm_random_seeds(count, s.ptr, s.len);
  freeString(s);
  return readWasmString(wasm.wasm_meta_ptr(), wasm.wasm_meta_len()).split("\n");
}

function syncModeUI() {
  document.body.classList.toggle("mode-grid", view === "grid");
  modeSingle.classList.toggle("active", view === "single");
  modeGrid.classList.toggle("active", view === "grid");
}

function syncNav() {
  btnBack.disabled = hIdx <= 0;
  btnFwd.disabled = hIdx >= history.length - 1;
}

function sameState(a, b) {
  if (!a || !b || a.mode !== b.mode) return false;
  if (a.mode === "single") return a.seed === b.seed;
  return a.n === b.n && a.seeds.join("|") === b.seeds.join("|");
}

function pushState(entry) {
  if (sameState(history[hIdx], entry)) { syncNav(); return; }
  history = history.slice(0, hIdx + 1);
  history.push(entry);
  if (history.length > 50) history.shift();
  hIdx = history.length - 1;
  syncNav();
}

function renderSingle(seed, animate = false) {
  const seq = ++renderSeq;
  view = "single";
  syncModeUI();
  stage.style.aspectRatio = "760 / 880";
  stage.style.width = "";
  stage.style.setProperty("--stage-ratio", 760 / 880);
  stage.onclick = null;
  currentSeed = seed;
  seedInput.value = seed;
  lastSingleSeed = seed;
  try { window.history.replaceState(null, "", "?seed=" + encodeURIComponent(seed)); } catch (e) {}
  clearDrawTimers();
  const res = generateShellSVG(seed);
  if (seq !== renderSeq) return;
  currentSvgString = res.svg;
  stage.innerHTML = res.svg;

  metaBase = res.meta;
  metaInfo.textContent = metaBase;

  if (animate) animateDrawing();
}

async function renderGridEntry(entry, animate = false) {
  const seq = ++renderSeq;
  const n = entry.n;
  const CELL_W = 760, CELL_H = 880, GAP = 12;
  const sw = CELL_W / 2, sh = CELL_H / 2;
  const W = n * sw + (n + 1) * GAP;
  const H = n * sh + (n + 1) * GAP;

  view = "grid";
  syncModeUI();
  clearDrawTimers();
  stage.style.aspectRatio = `${W} / ${H}`;
  stage.style.width = "min(calc(100vw - 170px), calc((100vh - 165px) * " + (W / H) + "))";
  stage.style.setProperty("--stage-ratio", W / H);
  gridSize.value = String(n);
  lastGridState = entry;
  currentSeed = `grid-${n}x${n}`;
  try { window.history.replaceState(null, "", "?grid=" + entry.n); } catch (e) {}

  if (entry.svg) {
    entry.W = entry.W || W;
    entry.H = entry.H || H;
    currentSvgString = entry.svg;
    stage.innerHTML = entry.svg;
    metaBase = "click a shell to open";
    metaInfo.textContent = metaBase;
    stage.onclick = onGridClick;
    if (animate) animateDrawing();
    return;
  }

  stage.onclick = null;
  metaInfo.textContent = "generating grid...";
  const built = await buildGridParallel(n, entry.seeds);
  entry.svg = built.svg;
  entry.W = built.W;
  entry.H = built.H;
  if (seq !== renderSeq) return;
  currentSvgString = entry.svg;
  stage.innerHTML = entry.svg;
  metaBase = "click a shell to open";
  metaInfo.textContent = metaBase;
  stage.onclick = onGridClick;
  if (animate) animateDrawing();
}

async function applyState(entry, animate = false) {
  if (!entry) return;
  if (entry.mode === "single") {
    renderSingle(entry.seed, animate);
    lastSingleSeed = entry.seed;
    seedInput.value = entry.seed;
    view = "single";
    syncModeUI();
  } else {
    renderGridEntry(entry, animate);
    lastGridState = entry;
    gridSize.value = String(entry.n);
    view = "grid";
    syncModeUI();
  }
  syncNav();
}

const clampN = (n) => Math.min(4, Math.max(2, parseInt(n, 10) || 3));
const selN = () => clampN(gridSize.value);

function randomSingleSeed() {
  return randomSeeds(1)[0];
}

function randomGridSeeds(n) {
  return randomSeeds(n * n);
}

function makeGridEntry(n, seeds) {
  n = clampN(n);
  return { mode: "grid", n, seeds: seeds ? seeds.slice() : randomGridSeeds(n) };
}

// create a fresh grid, then commit it to the unified timeline
function runGrid(n, seeds) {
  const entry = makeGridEntry(n, seeds);
  pushState(entry);
  applyState(entry);
}

function onGridClick(ev) {
  const g = ev.target.closest("[data-seed]");
  if (!g) return;
  const seed = g.getAttribute("data-seed");
  if (!seed) return;
  const entry = { mode: "single", seed };
  pushState(entry);
  applyState(entry);
}

let drawTimers = [];
function clearDrawTimers() { drawTimers.forEach(t => clearTimeout(t)); drawTimers = []; }

function finishDrawing() {
  clearDrawTimers();
  stage.querySelectorAll("#watercolor-layer .wash").forEach(p => {
    p.style.transition = "none"; p.style.opacity = "1";
  });
  stage.querySelectorAll("#ink-layer path").forEach(p => {
    p.style.transition = "none"; p.style.strokeDasharray = "none"; p.style.strokeDashoffset = "0";
  });
  if (view === "grid") stage.onclick = onGridClick;
  else stage.onclick = null;
  metaInfo.textContent = metaBase;
}

function animateDrawing() {
  clearDrawTimers();
  if (!currentSvgString) return;

  // Always restart from the clean markup so a mid-flight redraw cannot stack
  // transitions or leave a half-finished frame behind.
  stage.innerHTML = currentSvgString;

  const cellGroups = [...stage.querySelectorAll("g[data-seed]")];
  const roots = cellGroups.length > 0 ? cellGroups : [stage];

  // Initialize hidden starting states on all roots first.
  const perRoot = roots.map(root => {
    const washes = [...root.querySelectorAll("#watercolor-layer .wash")];
    const traces = [...root.querySelectorAll("#ink-prim path.trace"),
                    ...root.querySelectorAll("#ink-med path.trace")];
    const batches = [...root.querySelectorAll("#ink-fine path.trace-batch")];
    return { root, washes, traces, batches };
  });

  perRoot.forEach(({ washes, traces, batches }) => {
    washes.forEach(p => { p.style.transition = "none"; p.style.opacity = "0"; });
    traces.forEach(p => {
      let len = 300;
      try { len = p.getTotalLength() || 300; } catch (e) {}
      p.dataset.len = len;
      p.style.transition = "none";
      p.style.strokeDasharray = `${len} ${len}`;
      p.style.strokeDashoffset = `${len}`;
    });
    batches.forEach(p => { p.style.transition = "none"; p.style.opacity = "0"; });
  });

  // force a single DOM reflow so the start states apply before transitions
  void stage.offsetWidth;

  // Each root animates independently in parallel, starting at t = 0.
  perRoot.forEach(({ washes, traces, batches }) => {
    // blotches appear one at a time, in paint order
    washes.forEach((p, i) => {
      drawTimers.push(setTimeout(() => {
        p.style.transition = "opacity 0.3s ease-out";
        p.style.opacity = "1";
      }, i * 30));
    });
    let t = washes.length * 30 + 300;

    // ink traces one stroke at a time: seams, then rings, then fine batches
    const queue = [...traces, ...batches];
    queue.forEach(p => {
      const isBatch = p.classList.contains("trace-batch");
      const at = t;
      if (isBatch) {
        drawTimers.push(setTimeout(() => {
          p.style.transition = "opacity 0.4s ease-out";
          p.style.opacity = "1";
        }, at));
        t += 120;
      } else {
        const len = parseFloat(p.dataset.len || "300");
        const dur = Math.min(0.5, Math.max(0.08, len / 800));
        drawTimers.push(setTimeout(() => {
          p.style.transition = `stroke-dashoffset ${dur}s cubic-bezier(0.25, 0.1, 0.25, 1)`;
          p.style.strokeDashoffset = "0";
        }, at));
        t += dur * 1000 * 0.4;
      }
    });
  });

  metaInfo.textContent = metaBase + " (drawing... click art to finish)";  stage.onclick = () => finishDrawing();
}

function generateSingleFromInput() {
  let s = seedInput.value.trim().slice(0, 60);  if (!s) s = randomSingleSeed();
  const entry = { mode: "single", seed: s };
  pushState(entry);
  applyState(entry);
}

function bindEvents() {
  btnGenerate.addEventListener("click", () => {
    if (view !== "single") return;
    generateSingleFromInput();
  });

  btnRandom.addEventListener("click", () => {
    if (view === "grid") {
      runGrid(selN());
    } else {
      const entry = { mode: "single", seed: randomSingleSeed() };
      pushState(entry);
      applyState(entry);
    }
  });

  btnAnimate.addEventListener("click", () => {
    // redraw replays the drawing process for the current shell/grid, never new
    animateDrawing();
  });

  btnBack.addEventListener("click", () => {
    if (hIdx > 0) {
      hIdx -= 1;
      applyState(history[hIdx], false);
    }
  });

  btnFwd.addEventListener("click", () => {
    if (hIdx < history.length - 1) {
      hIdx += 1;
      applyState(history[hIdx], false);
    }
  });

  modeSingle.addEventListener("click", () => {
    if (view === "single") return;
    const seed = lastSingleSeed != null ? lastSingleSeed : randomSingleSeed();
    const entry = { mode: "single", seed };
    pushState(entry);
    applyState(entry);
  });

  modeGrid.addEventListener("click", () => {
    if (view === "grid") return;
    const entry = lastGridState || makeGridEntry(selN());
    pushState(entry);
    applyState(entry);
  });

  gridSize.addEventListener("change", () => {
    runGrid(selN());
  });

  btnDownload.addEventListener("click", () => {
    if (!currentSvgString) return;
    const blob = new Blob([currentSvgString], { type: "image/svg+xml;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    if (view === "grid") {
      const n = lastGridState ? lastGridState.n : selN();
      a.download = `shellmesh_grid_${n}x${n}.svg`;
    } else {
      const slug = currentSeed.slice(0, 40).replace(/[^a-z0-9_-]/gi, "_").toLowerCase() || "shell";
      a.download = `shellmesh_${slug}.svg`;
    }
    a.click();
    URL.revokeObjectURL(url);
  });

  btnDownloadPng.addEventListener("click", () => {
    if (!currentSvgString) return;
    let W, H;
    if (view === "grid") {
      W = lastGridState ? lastGridState.W : 760;
      H = lastGridState ? lastGridState.H : 880;
    } else {
      W = 760;
      H = 880;
    }
    const canvas = document.createElement("canvas");
    canvas.width = W * 2;
    canvas.height = H * 2;
    const ctx = canvas.getContext("2d");
    const sizedSvg = currentSvgString.replace(
      /^<svg([^>]*?)width="[^"]*"([^>]*?)height="[^"]*"/,
      `<svg$1width="${W * 2}"$2height="${H * 2}"`
    );
    const blob = new Blob([sizedSvg], { type: "image/svg+xml;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const img = new Image(W * 2, H * 2);
    img.onload = () => {
      ctx.drawImage(img, 0, 0, W * 2, H * 2);
      URL.revokeObjectURL(url);
      canvas.toBlob((pngBlob) => {
        const pngUrl = URL.createObjectURL(pngBlob);
        const a = document.createElement("a");
        a.href = pngUrl;
        if (view === "grid") {
          const n = lastGridState ? lastGridState.n : selN();
          a.download = `shellmesh_grid_${n}x${n}.png`;
        } else {
          const slug = currentSeed.slice(0, 40).replace(/[^a-z0-9_-]/gi, "_").toLowerCase() || "shell";
          a.download = `shellmesh_${slug}.png`;
        }
        a.click();
        URL.revokeObjectURL(pngUrl);
      }, "image/png");
    };
    img.src = url;
  });

  btnCredits.addEventListener("click", () => {
    creditsModal.hidden = false;
  });
  btnCloseCredits.addEventListener("click", () => {
    creditsModal.hidden = true;
  });
  creditsModal.addEventListener("click", (e) => {
    if (e.target === creditsModal) creditsModal.hidden = true;
  });

  seedInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && view === "single") generateSingleFromInput();
  });

  window.addEventListener("keydown", (e) => {
    if (!creditsModal.hidden) {
      if (e.key === "Escape") {
        creditsModal.hidden = true;
      }
      return;
    }
    if (document.activeElement === seedInput) return;
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (e.key === " " || e.key === "r" || e.key === "R") {
      e.preventDefault();
      btnRandom.click();
    } else if (e.key === "ArrowLeft") {
      if (!btnBack.disabled) btnBack.click();
    } else if (e.key === "ArrowRight") {
      if (!btnFwd.disabled) btnFwd.click();
    } else if (e.key === "d" || e.key === "D") {
      btnAnimate.click();
    } else if (e.key === "g" || e.key === "G") {
      (view === "single" ? modeGrid : modeSingle).click();
    } else if (e.key === "Escape") {
      if (drawTimers.length > 0) finishDrawing();
    }
  });
}

// Initial render: ?seed= in the URL reproduces an exact shell, ?grid=N opens
// a fresh N×N grid, else the fixed initial seed
async function start() {
  try {
    wasm = await loadWasm();
  } catch (err) {
    metaInfo.textContent = "failed to load pkg/shellmesh.wasm: " + err.message;
    return;
  }
  bindEvents();

  const urlParams = new URLSearchParams(location.search);
  const urlSeed = urlParams.get("seed");
  const urlGrid = parseInt(urlParams.get("grid") || "", 10);
  if (urlSeed) {
    const entry = { mode: "single", seed: urlSeed };
    pushState(entry);
    applyState(entry);
  } else if (urlGrid >= 2 && urlGrid <= 4) {
    runGrid(urlGrid);
  } else {
    const entry = { mode: "single", seed: randomSeeds(1, INIT_SEED)[0] };
    pushState(entry);
    applyState(entry);
  }
}

start();
