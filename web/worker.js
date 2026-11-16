// Converts one file at a time with the Rust engine compiled to WebAssembly.
// A parser panic aborts the whole module, so the page replaces a crashed worker.
import init, { convert } from "./pkg/anytomd_wasm.js";

const ready = init();

self.onmessage = async e => {
  const { id, name, buf } = e.data;
  try {
    await ready;
  } catch (err) {
    self.postMessage({ id, error: "failed:The converter could not start in this browser.", fatal: true });
    return;
  }
  try {
    self.postMessage({ id, md: convert(name, new Uint8Array(buf)) });
  } catch (err) {
    if (typeof err === "string") self.postMessage({ id, error: err });
    else self.postMessage({ id, error: "failed:File appears to be damaged (the converter crashed)", crashed: true });
  }
};
