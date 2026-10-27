import { describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { ping } from "./ipc";

describe("ping", () => {
  it("calls the Rust command and returns its answer", async () => {
    invoke.mockResolvedValueOnce({ message: "pong", version: "2.0.0" });
    await expect(ping()).resolves.toEqual({ message: "pong", version: "2.0.0" });
    expect(invoke).toHaveBeenCalledWith("ping");
  });
});
