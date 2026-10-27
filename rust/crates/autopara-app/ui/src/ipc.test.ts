import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import { importTimetable, markLesson, moveLesson, ping, selectGroup } from "./ipc";

describe("command wrappers", () => {
  beforeEach(() => invoke.mockReset());

  it("ping calls the Rust command and returns its answer", async () => {
    invoke.mockResolvedValueOnce({ message: "pong", version: "2.0.0" });
    await expect(ping()).resolves.toEqual({ message: "pong", version: "2.0.0" });
    expect(invoke).toHaveBeenCalledWith("ping");
  });

  it("passes arguments in camelCase, as Tauri maps them to the Rust parameters", async () => {
    await selectGroup(4);
    await importTimetable("C:/t.docx", 1, "21 група");
    await markLesson(7, "2026-10-05", "skipped");
    expect(invoke.mock.calls).toEqual([
      ["select_group", { groupId: 4 }],
      ["import_timetable", { path: "C:/t.docx", ordinal: 1, groupName: "21 група" }],
      ["mark_lesson", { lessonId: 7, date: "2026-10-05", status: "skipped" }],
    ]);
  });

  it("sends a move as one request object", async () => {
    const request = { lesson_id: 7, from: "2026-10-05", to: "2026-10-06", start: "11:20", scope: "only_this_date" } as const;
    await moveLesson(request);
    expect(invoke).toHaveBeenCalledWith("move_lesson", { request });
  });
});
