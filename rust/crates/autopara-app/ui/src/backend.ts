// Everything the interface asks of the outside world, behind one interface: the real app talks to
// Rust over IPC; the demo (`?demo`) answers from the sample timetable with a pinned clock, so the
// interface can be developed, tested and photographed in an ordinary browser.
import type { AssistSettings } from "./bindings/AssistSettings";
import type { CoursePreview } from "./bindings/CoursePreview";
import type { CourseSummary } from "./bindings/CourseSummary";
import type { Lesson } from "./bindings/Lesson";
import type { LessonInput } from "./bindings/LessonInput";
import type { MoveRequest } from "./bindings/MoveRequest";
import type { Occurrence } from "./bindings/Occurrence";
import type { OccurrenceStatus } from "./bindings/OccurrenceStatus";
import type { SchedulerEvent } from "./bindings/SchedulerEvent";
import type { Settings } from "./bindings/Settings";
import type { SettingsInput } from "./bindings/SettingsInput";
import type { Theme } from "./bindings/Theme";
import type { UpdateStatus } from "./bindings/UpdateStatus";
import type { ViewMode } from "./bindings/ViewMode";
import * as ipc from "./ipc";
import { type IsoDate, addDays, addMinutes, toMinutes, weekday } from "./lib/calendar";

/** Lecture Assist, straight from IPC in the app; a stand-in in the demo. */
export type AssistBackend = Pick<
  typeof ipc,
  | "assistSettings"
  | "saveAssistSettings"
  | "setAssistConsent"
  | "setAssistHotkey"
  | "assistHotkeyActive"
  | "assistTestConnection"
  | "assistDevices"
  | "assistModels"
  | "assistDownloadModel"
  | "assistDeleteModel"
  | "assistDeleteAll"
  | "assistStart"
  | "assistStop"
  | "onAssist"
  | "onAssistDownload"
>;

export interface Backend {
  /** The current local time. The demo pins it. */
  now(): Date;
  version(): Promise<string>;
  isMac(): boolean;
  getSettings(): Promise<Settings>;
  saveSettings(input: SettingsInput): Promise<Settings>;
  setView(view: ViewMode): Promise<void>;
  setTheme(theme: Theme): Promise<void>;
  listCourses(): Promise<CourseSummary[]>;
  selectGroup(groupId: number): Promise<void>;
  groupLessons(groupId: number): Promise<Lesson[]>;
  occurrences(from: IsoDate, to: IsoDate): Promise<Occurrence[]>;
  previewTimetable(path: string): Promise<CoursePreview[]>;
  importTimetable(path: string, ordinal: number, groupName: string): Promise<void>;
  pickTimetable(): Promise<string | null>;
  saveLesson(input: LessonInput): Promise<number>;
  moveLesson(request: MoveRequest): Promise<void>;
  deleteLesson(lessonId: number): Promise<void>;
  openLesson(lessonId: number, date: IsoDate): Promise<boolean>;
  markLesson(lessonId: number, date: IsoDate, status: OccurrenceStatus): Promise<void>;
  clearMark(lessonId: number, date: IsoDate): Promise<void>;
  diagnostics(): Promise<string>;
  checkUpdates(): Promise<void>;
  downloadUpdate(): Promise<void>;
  installUpdate(): Promise<void>;
  skipUpdate(version: string): Promise<void>;
  /** A release page, in the system browser. */
  openPage(url: string): Promise<void>;
  onUpdate(handler: (status: UpdateStatus) => void): void;
  onSchedulerEvent(handler: (event: SchedulerEvent) => void): void;
  onTrayAction(handler: (action: string) => void): void;
  /** A file dropped anywhere on the window: its path, while dragging (`hover`) and on drop. */
  onFileDrop(handler: (kind: "hover" | "leave" | "drop", paths: string[]) => void): void;
  assist: AssistBackend;
}

// ------------------------------------------------------------------- the real app

async function tauriFileDrop(handler: (kind: "hover" | "leave" | "drop", paths: string[]) => void) {
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  await getCurrentWebview().onDragDropEvent(({ payload }) => {
    if (payload.type === "enter" || payload.type === "over") handler("hover", "paths" in payload ? payload.paths : []);
    else if (payload.type === "drop") handler("drop", payload.paths);
    else handler("leave", []);
  });
}

export function tauriBackend(): Backend {
  return {
    now: () => new Date(),
    version: async () => (await ipc.appInfo()).version,
    isMac: () => navigator.userAgent.includes("Mac"),
    getSettings: ipc.getSettings,
    saveSettings: ipc.saveSettings,
    setView: ipc.setView,
    setTheme: ipc.setTheme,
    listCourses: ipc.listCourses,
    selectGroup: ipc.selectGroup,
    groupLessons: ipc.groupLessons,
    occurrences: ipc.occurrences,
    previewTimetable: ipc.previewTimetable,
    importTimetable: ipc.importTimetable,
    pickTimetable: ipc.pickTimetable,
    saveLesson: ipc.saveLesson,
    moveLesson: ipc.moveLesson,
    deleteLesson: ipc.deleteLesson,
    openLesson: ipc.openLesson,
    markLesson: ipc.markLesson,
    clearMark: ipc.clearMark,
    diagnostics: ipc.diagnostics,
    checkUpdates: ipc.checkUpdates,
    downloadUpdate: ipc.downloadUpdate,
    installUpdate: ipc.installUpdate,
    skipUpdate: ipc.skipUpdate,
    openPage: ipc.openReleasePage,
    onUpdate: (handler) => void ipc.onUpdate(handler),
    onSchedulerEvent: (handler) => void ipc.onSchedulerEvent(handler),
    onTrayAction: (handler) => void ipc.onTrayAction(handler),
    onFileDrop: (handler) => void tauriFileDrop(handler),
    assist: ipc,
  };
}

// ------------------------------------------------------------------------ the demo

/** The moment the reference pictures are taken: a Monday, in the middle of the second class. */
export const DEMO_NOW = new Date(2026, 2, 2, 9, 40, 20);

interface DemoData {
  settings: Settings;
  courses: CourseSummary[];
  lessons: Record<string, Lesson[]>;
}

export async function demoBackend(params: URLSearchParams): Promise<Backend> {
  const data = (await import("./demo/sample.json")).default as unknown as DemoData;
  const settings: Settings = { ...data.settings };
  const theme = params.get("theme");
  if (theme === "light" || theme === "dark") settings.theme = theme;
  const view = params.get("view");
  if (view === "day" || view === "week" || view === "month") settings.view_mode = view;
  const lessons = new Map(Object.entries(data.lessons).map(([id, list]) => [Number(id), [...list]]));
  const marks = new Map<string, Occurrence>();
  let nextId = 1000;
  const find = (id: number) => [...lessons.values()].flat().find((lesson) => lesson.id === id);

  return {
    now: () => new Date(DEMO_NOW),
    version: async () => "2.0.0-demo",
    isMac: () => false,
    getSettings: async () => ({ ...settings }),
    saveSettings: async (input) => Object.assign(settings, input),
    setView: async (view) => void (settings.view_mode = view),
    setTheme: async (value) => void (settings.theme = value),
    listCourses: async () => data.courses,
    selectGroup: async (groupId) => {
      const course = data.courses.find((c) => c.groups.some((g) => g.group.id === groupId));
      settings.selected_group_id = groupId;
      settings.selected_course_id = course?.course.id ?? null;
    },
    groupLessons: async (groupId) => lessons.get(groupId) ?? [],
    occurrences: async (from, to) => [...marks.values()].filter((o) => from <= o.date && o.date <= to),
    previewTimetable: async () =>
      data.courses.map(({ course, groups }) => ({
        ordinal: course.ordinal,
        name: course.name,
        lessons: groups.reduce((sum, g) => sum + g.lessons, 0),
        groups: groups.map(({ group, lessons: count }) => ({
          name: group.name,
          specialty: group.specialty,
          lessons: count,
          shared: 0,
          without_link: 0,
        })),
      })),
    importTimetable: async () => undefined,
    pickTimetable: async () => "C:/Users/demo/Desktop/розклад.docx",
    saveLesson: async (input) => {
      const group = settings.selected_group_id ?? 1;
      const list = lessons.get(group) ?? [];
      const existing = input.id === null ? undefined : find(input.id);
      const lesson: Lesson = {
        id: existing?.id ?? nextId++,
        course_id: settings.selected_course_id ?? 1,
        weekday: input.recurrence.kind === "once" ? weekday(input.recurrence.date) : input.recurrence.kind === "weekly_until" ? weekday(input.recurrence.from) : input.weekday,
        pair: 1,
        start: input.start,
        end: input.end,
        subject: input.subject.trim(),
        teacher: input.teacher ? `(${input.teacher.replace(/^\(|\)$/g, "")})` : "",
        url: input.url,
        provider: input.url?.includes("zoom") ? "zoom" : input.url?.includes("meet.google") ? "google_meet" : "unknown",
        needs_link: !input.url,
        is_manual: existing?.is_manual ?? true,
        source_key: "",
        group_names: existing?.group_names ?? [],
        recurrence: existing && !existing.is_manual ? existing.recurrence : input.recurrence,
        skip_dates: existing?.skip_dates ?? [],
      };
      if (existing) Object.assign(existing, lesson);
      else list.push(lesson);
      lessons.set(group, list);
      return lesson.id;
    },
    moveLesson: async (request) => {
      const lesson = find(request.lesson_id);
      if (!lesson) return;
      const length = toMinutes(lesson.end) - toMinutes(lesson.start);
      if (request.scope === "only_this_date" && request.from) {
        lesson.skip_dates = [...lesson.skip_dates, request.from];
        const copy: Lesson = { ...lesson, id: nextId++, is_manual: true, weekday: weekday(request.to), recurrence: { kind: "once", date: request.to }, skip_dates: [] };
        copy.start = request.start;
        copy.end = addMinutes(request.start, length);
        lessons.get(settings.selected_group_id ?? 1)?.push(copy);
        return;
      }
      const shift = request.from ? Math.round((Date.parse(request.to) - Date.parse(request.from)) / 864e5) : 0;
      lesson.start = request.start;
      lesson.end = addMinutes(request.start, length);
      lesson.weekday = weekday(request.to);
      const r = lesson.recurrence;
      if (r.kind === "once") lesson.recurrence = { kind: "once", date: request.to };
      if (r.kind === "weekly_until") lesson.recurrence = { kind: "weekly_until", from: request.to, until: addDays(r.until, shift) };
    },
    deleteLesson: async (lessonId) => {
      for (const [group, list] of lessons) lessons.set(group, list.filter((lesson) => lesson.id !== lessonId));
    },
    openLesson: async (lessonId, date) => {
      marks.set(`${lessonId}|${date}`, { id: 0, lesson_id: lessonId, date, status: "manual", fired_at: "2026-03-02T09:40:20" });
      return true;
    },
    markLesson: async (lessonId, date, status) => {
      marks.set(`${lessonId}|${date}`, { id: 0, lesson_id: lessonId, date, status, fired_at: "2026-03-02T09:40:20" });
    },
    clearMark: async (lessonId, date) => void marks.delete(`${lessonId}|${date}`),
    diagnostics: async () => "AutoPara demo",
    checkUpdates: async () => undefined,
    downloadUpdate: async () => undefined,
    installUpdate: async () => undefined,
    skipUpdate: async () => undefined,
    openPage: async () => undefined,
    onUpdate: () => undefined,
    onSchedulerEvent: () => undefined,
    onTrayAction: () => undefined,
    onFileDrop: () => undefined,
    assist: demoAssist(),
  };
}

/** Consent given, one model installed, no chat provider: enough to show the screens. */
function demoAssist(): AssistBackend {
  let settings: AssistSettings = {
    consent: true, device: "", model: "small", language: "auto", glossary: "", provider: "", base_url: "",
    chat_model: "", fallback: true, retention_days: 30, auto_listen: false, has_key: false, hotkey: "Ctrl+Alt+L",
  };
  const model = (id: string, size: number, vad = false) => ({ id, file: `${id}.bin`, url: "", size, sha256: "", vad });
  const unlisten = async () => () => undefined;
  return {
    assistSettings: async () => settings,
    saveAssistSettings: async (input) => (settings = { ...input }),
    setAssistConsent: async (accepted) => void (settings.consent = accepted),
    setAssistHotkey: async (hotkey) => void (settings.hotkey = hotkey),
    assistHotkeyActive: async () => true,
    assistTestConnection: async () => undefined,
    assistDevices: async () => [{ id: "speakers", name: "Динаміки", loopback: true }],
    assistModels: async () => [
      { model: model("small", 487_601_967), installed: true },
      { model: model("large-v3-turbo", 1_624_555_275), installed: false },
      { model: model("silero", 885_098, true), installed: true },
    ],
    assistDownloadModel: async () => undefined,
    assistDeleteModel: async () => undefined,
    assistDeleteAll: async () => undefined,
    assistStart: async () => undefined,
    assistStop: async () => undefined,
    onAssist: unlisten,
    onAssistDownload: unlisten,
  };
}
