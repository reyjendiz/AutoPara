import { describe, expect, it } from "vitest";
import type { Lesson } from "../bindings/Lesson";
import {
  addMinutes,
  dayTitle,
  groupsWord,
  lengthText,
  monthGrid,
  pairsWord,
  shiftMonth,
  timesWord,
  weekTitle,
  weekday,
} from "./calendar";
import { md5, subjectColor, tint } from "./color";
import { cardBox, chooseLayout, nowOffset, spanFor } from "./grid";
import { findMatch, fold, matches, nextUp, nextUpLabel, occursOn, repeats, subjects } from "./lessons";

function lesson(over: Partial<Lesson> = {}): Lesson {
  return {
    id: 1,
    course_id: 1,
    weekday: 0,
    pair: 2,
    start: "09:30",
    end: "10:50",
    subject: "Загальна педагогіка",
    teacher: "(Роман Н.М.)",
    url: "https://zoom.us/j/1",
    provider: "zoom",
    needs_link: false,
    is_manual: false,
    source_key: "",
    group_names: ["11"],
    recurrence: { kind: "weekly" },
    skip_dates: [],
    ...over,
  };
}

describe("calendar", () => {
  it("counts weekdays from Monday", () => {
    expect(weekday("2026-10-05")).toBe(0);
    expect(weekday("2026-10-11")).toBe(6);
  });

  it("titles a week the way the Python app does", () => {
    expect(weekTitle("2026-10-05")).toEqual(["5–11 жовтня", "2026"]);
    expect(weekTitle("2026-09-28")).toEqual(["28 вересня – 4 жовтня", "2026"]);
    expect(weekTitle("2026-12-28")).toEqual(["28 грудня 2026 – 3 січня", "2027"]);
    expect(dayTitle("2026-03-02")).toEqual(["Понеділок, 2 березня", "2026"]);
  });

  it("moves by months and clamps to the shorter month", () => {
    expect(shiftMonth("2026-01-31", 1)).toBe("2026-02-28");
    expect(shiftMonth("2026-01-15", -1)).toBe("2025-12-15");
    const grid = monthGrid("2026-03-17");
    expect([grid.length, grid[0], grid[41]]).toEqual([42, "2026-02-23", "2026-04-05"]);
  });

  it("clamps time arithmetic at the end of the day", () => {
    expect(addMinutes("23:00", 80)).toBe("23:59");
    expect(lengthText(80)).toBe("1 год 20 хв");
    expect(lengthText(45)).toBe("45 хв");
  });

  it("speaks Ukrainian plurals", () => {
    expect([1, 2, 5, 11, 12, 21, 22, 25].map((n) => `${n} ${pairsWord(n)}`)).toEqual([
      "1 пара", "2 пари", "5 пар", "11 пар", "12 пар", "21 пара", "22 пари", "25 пар",
    ]);
    expect([2, 5, 12].map(groupsWord)).toEqual(["групи", "груп", "груп"]);
    expect([1, 3, 7].map(timesWord)).toEqual(["раз", "рази", "разів"]);
  });
});

describe("grid geometry", () => {
  it("places a class by its real time", () => {
    expect(spanFor(570, 650)).toEqual({ row: 1, span: 2, top: 30, bottom: 10 });
    expect(cardBox(570, 650)).toEqual({ top: 99, height: 88 });
  });

  it("clamps to the visible day and keeps short classes readable", () => {
    expect(spanFor(420, 470)).toEqual({ row: 0, span: 1, top: 0, bottom: 34 });
    expect(spanFor(1200, 1300).row).toBe(10);
    expect(nowOffset(580)).toBe(110);
    expect(nowOffset(1140)).toBeNull();
  });

  it("drops the time first, then the teacher, when a card is short", () => {
    const lines = { chips: 18, subject: 16, teacher: 14, time: 14 };
    expect(chooseLayout(200, lines)).toEqual({ subjectLines: 3, teacher: true, time: true, detail: true });
    // 2 subject lines + teacher + time need 92 px; at 88 the time goes, at 70 the teacher too.
    expect(chooseLayout(100, lines)).toEqual({ subjectLines: 2, teacher: true, time: true, detail: false });
    expect(chooseLayout(88, lines)).toEqual({ subjectLines: 2, teacher: true, time: false, detail: false });
    expect(chooseLayout(70, lines)).toEqual({ subjectLines: 2, teacher: false, time: false, detail: false });
    expect(chooseLayout(20, lines)).toEqual({ subjectLines: 1, teacher: false, time: false, detail: false });
  });
});

describe("lessons", () => {
  it("knows which dates a lesson happens on", () => {
    expect(occursOn(lesson(), "2026-10-05")).toBe(true);
    expect(occursOn(lesson(), "2026-10-06")).toBe(false);
    expect(occursOn(lesson({ skip_dates: ["2026-10-05"] }), "2026-10-05")).toBe(false);
    const series = lesson({ recurrence: { kind: "weekly_until", from: "2026-10-05", until: "2026-10-19" } });
    expect(["2026-09-28", "2026-10-12", "2026-10-26"].map((d) => occursOn(series, d))).toEqual([false, true, false]);
    expect(repeats(lesson({ recurrence: { kind: "once", date: "2026-10-05" } }))).toBe(false);
  });

  it("searches every word in subject and teacher, ignoring case and apostrophes", () => {
    expect(matches(lesson(), { text: "роман ПЕДАГ", subject: "" })).toBe(true);
    expect(matches(lesson(), { text: "логіка", subject: "" })).toBe(false);
    expect(matches(lesson({ subject: "П`ятничний" }), { text: "п'ятн", subject: "" })).toBe(true);
    expect(fold("  Straße ")).toBe("strasse");
  });

  it("lists subjects once, in Ukrainian order", () => {
    const list = [lesson({ subject: "Історія" }), lesson({ subject: "Арт" }), lesson({ subject: "історія" }), lesson({ subject: "Біологія" })];
    expect(subjects(list)).toEqual(["Арт", "Біологія", "Історія"]);
  });

  it("finds the next date with a match", () => {
    expect(findMatch([lesson()], { text: "педаг", subject: "" }, "2026-10-05", 1)).toBe("2026-10-12");
    expect(findMatch([lesson()], { text: "nothing", subject: "" }, "2026-10-05", 1)).toBeNull();
  });

  it("says what is on or next", () => {
    const on = nextUp([lesson()], "2026-03-02", 580, new Set());
    expect(on && nextUpLabel(on)).toBe("Зараз · Загальна педагогіка · ще 70 хв");
    const next = nextUp([lesson()], "2026-03-02", 545, new Set());
    expect(next && nextUpLabel(next)).toBe("Далі · Загальна педагогіка · через 25 хв");
    const later = nextUp([lesson()], "2026-03-02", 420, new Set());
    expect(later && nextUpLabel(later)).toBe("Далі · Загальна педагогіка · о 09:30");
    expect(nextUp([lesson()], "2026-03-02", 545, new Set([1]))).toBeNull();
  });
});

describe("subject colours", () => {
  it("computes MD5", () => {
    expect(md5(new Uint8Array())).toBe("d41d8cd98f00b204e9800998ecf8427e");
    expect(md5(new TextEncoder().encode("abc"))).toBe("900150983cd24fb0d6963f7d28e17f72");
  });

  it("picks the same colour as the Python app", () => {
    expect(subjectColor("Історія України")).toBe("#ec6b66");
    expect(subjectColor("Загальна педагогіка")).toBe("#a8bdd6");
    expect(subjectColor("Іноземна мова")).toBe("#86cdf7");
    expect(subjectColor(" ПСИХОЛОГІЯ ")).toBe("#8566e6");
    expect(subjectColor("Straße")).toBe("#ec6b66");
  });

  it("tints a card", () => {
    expect(tint("#ffffff", "#ec6b66", 0.16)).toBe("#fce7e7"); // Python: QColor mix, same result
  });
});
