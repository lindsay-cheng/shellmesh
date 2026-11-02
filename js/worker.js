let wasm = null;
const encoder = new TextEncoder();
const decoder = new TextDecoder();

self.onmessage = async (e) => {
  const msg = e.data;
  if (msg.type === "init") {
    try {
      if (msg.module) {
        const instance = await WebAssembly.instantiate(msg.module, {});
        wasm = instance.exports;
      } else if (msg.url) {
        const res = await fetch(msg.url);
        const buf = await res.arrayBuffer();
        const instance = await WebAssembly.instantiate(buf, {});
        wasm = instance.exports;
      }
      postMessage({ type: "ready" });
    } catch (err) {
      postMessage({ type: "init_error", message: err.message });
    }
    return;
  }
  if (msg.type === "cell") {
    const { taskId, cellIdx, seed } = msg;
    try {
      const bytes = encoder.encode(seed);
      const ptr = wasm.wasm_alloc(bytes.length);
      new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
      wasm.wasm_generate_cell(cellIdx, ptr, bytes.length);
      wasm.wasm_dealloc(ptr, bytes.length);
      const inner = decoder.decode(new Uint8Array(wasm.memory.buffer, wasm.wasm_svg_ptr(), wasm.wasm_svg_len()));
      postMessage({ type: "cell_done", taskId, cellIdx, inner });
    } catch (err) {
      postMessage({ type: "cell_error", taskId, message: err.message });
    }
    return;
  }
};