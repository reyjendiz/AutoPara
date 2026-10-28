// Typed wrappers over the Rust commands. The types are generated from Rust (src/bindings); the
// argument names are the commands' parameters in camelCase, as Tauri expects them.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppInfo } from "./bindings/AppInfo";
import type { AskKind } from "./bindings/AskKind";
import type { AssistEvent } from "./bindings/AssistEvent";
import type { AssistSettings } from "./bindings/AssistSettings";
import type { AudioDevice } from "./bindings/AudioDevice";
import type { ModelStatus } from "./bindings/ModelStatus";
import type { CoursePreview } from "./bindings/CoursePreview";
import type { CourseSummary } from "./bindings/CourseSummary";
import type { Lesson } from "./bindings/Lesson";
import type { LessonInput } from "./bindings/LessonInput";
import type { MoveRequest } from "./bindings/MoveRequest";
import type { Occurrence } from "./bindings/Occurrence";
import type { OccurrenceStatus } from "./bindings/OccurrenceStatus";
import type { Pong } from "./bindings/Pong";
import type { SchedulerEvent } from "./bindings/SchedulerEvent";
import type { Settings } from "./bindings/Settings";
import type { SettingsInput } from "./bindings/SettingsInput";
import type { Theme } from "./bindings/Theme";
import type { UpdateStatus } from "./bindings/UpdateStatus";
import type { ViewMode } from "./bindings/ViewMode";

/** An ISO date, `YYYY-MM-DD`, as the Rust side reads and writes it. */
export type IsoDate = string;

export const ping = (): Promise<Pong> => invoke<Pong>("ping");
export const appInfo = (): Promise<AppInfo> => invoke<AppInfo>("app_info");

export const getSettings = (): Promise<Settings> => invoke<Settings>("get_settings");
export const saveSettings = (input: SettingsInput): Promise<Settings> =>
  invoke<Settings>("save_settings", { input });
export const setView = (view: ViewMode): Promise<void> => invoke("set_view", { view });
export const setTheme = (theme: Theme): Promise<void> => invoke("set_theme", { theme });

export const listCourses = (): Promise<CourseSummary[]> => invoke<CourseSummary[]>("list_courses");
export const selectGroup = (groupId: number): Promise<void> => invoke("select_group", { groupId });
export const groupLessons = (groupId: number): Promise<Lesson[]> =>
  invoke<Lesson[]>("group_lessons", { groupId });
export const occurrences = (from: IsoDate, to: IsoDate): Promise<Occurrence[]> =>
  invoke<Occurrence[]>("occurrences", { from, to });

export const previewTimetable = (path: string): Promise<CoursePreview[]> =>
  invoke<CoursePreview[]>("preview_timetable", { path });
export const importTimetable = (path: string, ordinal: number, groupName: string): Promise<void> =>
  invoke("import_timetable", { path, ordinal, groupName });
export const pickTimetable = (): Promise<string | null> => invoke<string | null>("pick_timetable");

export const saveLesson = (input: LessonInput): Promise<number> => invoke<number>("save_lesson", { input });
export const moveLesson = (request: MoveRequest): Promise<void> => invoke("move_lesson", { request });
export const deleteLesson = (lessonId: number): Promise<void> => invoke("delete_lesson", { lessonId });

export const openLesson = (lessonId: number, date: IsoDate): Promise<boolean> =>
  invoke<boolean>("open_lesson", { lessonId, date });
export const markLesson = (lessonId: number, date: IsoDate, status: OccurrenceStatus): Promise<void> =>
  invoke("mark_lesson", { lessonId, date, status });
export const clearMark = (lessonId: number, date: IsoDate): Promise<void> =>
  invoke("clear_mark", { lessonId, date });

export const diagnostics = (): Promise<string> => invoke<string>("diagnostics");

export const checkUpdates = (): Promise<void> => invoke("check_updates");
export const downloadUpdate = (): Promise<void> => invoke("download_update");
export const installUpdate = (): Promise<void> => invoke("install_update");
export const skipUpdate = (version: string): Promise<void> => invoke("skip_update", { version });
export const openReleasePage = (url: string): Promise<void> => invoke("open_release_page", { url });

/** The updater's news: a version is out, downloading, ready, or nothing new. */
export const onUpdate = (handler: (status: UpdateStatus) => void): Promise<UnlistenFn> =>
  listen<UpdateStatus>("update", (event) => handler(event.payload));

/** Scheduler events: opened, missed, a class waiting for an answer, a reminder. */
export const onSchedulerEvent = (handler: (event: SchedulerEvent) => void): Promise<UnlistenFn> =>
  listen<SchedulerEvent>("scheduler", (event) => handler(event.payload));

/** The tray asked for a screen: `import` or `settings`. */
export const onTrayAction = (handler: (action: string) => void): Promise<UnlistenFn> =>
  listen<string>("tray-action", (event) => handler(event.payload));

// ------------------------------------------------------------------ Lecture Assist

export const assistSettings = (): Promise<AssistSettings> => invoke<AssistSettings>("assist_settings");
/** `apiKey`: `null` keeps the saved key, `""` forgets it. */
export const saveAssistSettings = (input: AssistSettings, apiKey: string | null): Promise<AssistSettings> =>
  invoke<AssistSettings>("save_assist_settings", { input, apiKey });
export const setAssistConsent = (accepted: boolean): Promise<void> => invoke("set_assist_consent", { accepted });
export const setAssistHotkey = (hotkey: string): Promise<void> => invoke("set_assist_hotkey", { hotkey });
export const assistHotkeyActive = (): Promise<boolean> => invoke<boolean>("assist_hotkey_active");
export const assistDevices = (): Promise<AudioDevice[]> => invoke<AudioDevice[]>("assist_devices");
export const assistModels = (): Promise<ModelStatus[]> => invoke<ModelStatus[]>("assist_models");
export const assistDownloadModel = (id: string): Promise<void> => invoke("assist_download_model", { id });
export const assistDeleteModel = (id: string): Promise<void> => invoke("assist_delete_model", { id });
export const assistDeleteAll = (): Promise<void> => invoke("assist_delete_all");
/** Errors: `consent_required`, `model_missing`. */
export const assistStart = (lessonId: number | null): Promise<void> => invoke("assist_start", { lessonId });
export const assistStop = (): Promise<void> => invoke("assist_stop");
export const assistPause = (): Promise<void> => invoke("assist_pause");
export const assistResume = (): Promise<void> => invoke("assist_resume");
export const assistAsk = (kind: AskKind, text: string): Promise<void> => invoke("assist_ask", { kind, text });
export const assistKeepListening = (): Promise<void> => invoke("assist_keep_listening");
export const assistSubject = (): Promise<string | null> => invoke<string | null>("assist_subject");

export const onAssist = (handler: (event: AssistEvent) => void): Promise<UnlistenFn> =>
  listen<AssistEvent>("assist", (event) => handler(event.payload));
/** Model download progress. */
export const onAssistDownload = (handler: (id: string, done: number, total: number) => void): Promise<UnlistenFn> =>
  listen<[string, number, number]>("assist-download", ({ payload: [id, done, total] }) => handler(id, done, total));
export const onAssistClassOver = (handler: () => void): Promise<UnlistenFn> =>
  listen("assist-class-over", () => handler());
