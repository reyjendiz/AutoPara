// Typed wrappers over the Rust commands. The types are generated from Rust (src/bindings).
import { invoke } from "@tauri-apps/api/core";
import type { Pong } from "./bindings/Pong";

export function ping(): Promise<Pong> {
  return invoke<Pong>("ping");
}
