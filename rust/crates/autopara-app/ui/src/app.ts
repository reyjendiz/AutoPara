// The window: state, reload and actions (Python `ui/main_window.py`). The database is the single
// source of truth: after every change the window asks the backend again and re-renders; the only
// state kept here is where the window is looking (the date, the view, the search).
import type { Backend } from "./backend";
import type { CourseSummary } from "./bindings/CourseSummary";
import type { Lesson } from "./bindings/Lesson";
import type { OccurrenceStatus } from "./bindings/OccurrenceStatus";
import type { Settings } from "./bindings/Settings";
import type { ViewMode } from "./bindings/ViewMode";
import { h, replace } from "./dom";
import { t } from "./i18n";
import {
  type IsoDate, addDays, addMinutes, firstOfMonth, monthGrid, shiftMonth, toIso, toMinutes, weekStart,
  weekday, longDate, dayName,
} from "./lib/calendar";
import { type LessonFilter, findMatch, isActive, matches, nextUp, nextUpLabel, occursOn, repeats, subjects } from "./lib/lessons";
import { CatchupBanner } from "./ui/banner";
import { UpdateBanner } from "./ui/updates";
import { FilterBar } from "./ui/filters";
import { renderGrid } from "./ui/grid";
import { Landing } from "./ui/landing";
import { renderMonth } from "./ui/month";
import { NavBar } from "./ui/nav";
import { ask, confirmYesNo, inform, showMenu } from "./ui/popups";
import { Rail } from "./ui/rail";
import { openEditor } from "./ui/dialogs/edit";
import { openGroupDialog } from "./ui/dialogs/group";
import { openImport } from "./ui/dialogs/import";
import { openSettings } from "./ui/dialogs/settings";
import { askConsent, openAssistSettings } from "./ui/dialogs/assist";

/** How a class looks on a date: what became of it, and whether it is the one to watch. */
export type Status = OccurrenceStatus | undefined;

export interface CardContext {
  status: Status;
  isNext: boolean;
}

/** A class on a date, as the views hand it back to the window. */
export interface ClassRef {
  lesson: Lesson;
  date: IsoDate;
}

const NEXT_UP_REFRESH_MS = 30_000;

export class App {
  settings!: Settings;
  courses: CourseSummary[] = [];
  lessons: Lesson[] = [];
  statuses = new Map<string, OccurrenceStatus>();
  anchor: IsoDate;
  filter: LessonFilter = { text: "", subject: "" };
  /** Another group of the chosen course, shown without changing the chosen one. */
  viewGroupId: number | null = null;
  private lastChosen: number | null = null;
  /** Morphs between views follow a class by its date; within one view, by its place. */
  morph: "place" | "date" = "place";

  readonly rail: Rail;
  readonly nav: NavBar;
  readonly filters: FilterBar;
  readonly catchup: CatchupBanner;
  readonly updates: UpdateBanner;
  readonly landing: Landing;
  readonly surface = h("div", { class: "surface-host" });
  private readonly content: HTMLElement;

  constructor(readonly backend: Backend, root: HTMLElement) {
    this.anchor = toIso(backend.now());
    this.rail = new Rail(this);
    this.nav = new NavBar(this);
    this.filters = new FilterBar(this);
    this.catchup = new CatchupBanner(this);
    this.updates = new UpdateBanner(this);
    this.landing = new Landing(this);
    this.content = h("main", { class: "content" }, this.nav.element, this.filters.element, this.updates.element, this.catchup.element, this.surface);
    replace(root, h("div", { class: "app" }, this.rail.element, this.content));
    this.surface.style.setProperty("display", "contents");
  }

  // ------------------------------------------------------------------ helpers

  now(): Date {
    return this.backend.now();
  }

  today(): IsoDate {
    return toIso(this.now());
  }

  nowMinutes(): number {
    const now = this.now();
    return now.getHours() * 60 + now.getMinutes();
  }

  statusOf(lesson: Lesson, date: IsoDate): Status {
    return this.statuses.get(`${lesson.id}|${date}`);
  }

  chosenGroupId(): number | null {
    return this.settings.selected_group_id;
  }

  shownGroupId(): number | null {
    const chosen = this.chosenGroupId();
    if (!chosen || !this.viewGroupId || this.viewGroupId === chosen) return chosen;
    const course = this.courseOf(chosen);
    return course?.groups.some((g) => g.group.id === this.viewGroupId) ? this.viewGroupId : chosen;
  }

  courseOf(groupId: number): CourseSummary | undefined {
    return this.courses.find((c) => c.groups.some((g) => g.group.id === groupId));
  }

  view(): ViewMode {
    return this.settings.view_mode;
  }

  shown(): Lesson[] {
    return isActive(this.filter) ? this.lessons.filter((lesson) => matches(lesson, this.filter)) : this.lessons;
  }

  /** The dates on screen. A week is Monday to Saturday, and Sunday only when a class is on it. */
  visibleDays(): IsoDate[] {
    const view = this.view();
    if (view === "day") return [this.anchor];
    if (view === "month") return monthGrid(this.anchor);
    const monday = weekStart(this.anchor);
    const days = Array.from({ length: 6 }, (_, i) => addDays(monday, i));
    const sunday = addDays(monday, 6);
    if (this.shown().some((lesson) => occursOn(lesson, sunday))) days.push(sunday);
    return days;
  }

  // ------------------------------------------------------------------ reload

  async start(): Promise<void> {
    this.settings = await this.backend.getSettings();
    this.applyTheme();
    window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => this.applyTheme());
    this.backend.onSchedulerEvent((event) => {
      if (event.kind === "catchup_available") {
        const lesson = this.lessons.find((l) => l.id === event.lesson_id);
        void this.offerCatchup(event.lesson_id, event.date, lesson);
      } else {
        if (event.kind === "opened") this.catchup.resolve(event.lesson_id);
        void this.reload();
      }
    });
    this.backend.onUpdate((status) => void this.updates.show(status));
    this.backend.onTrayAction((action) => {
      if (action.startsWith("assist:")) void this.assistRefused(action.slice("assist:".length), null);
      else void (action === "import" ? this.openImport() : this.openSettings());
    });
    void this.backend.assist.onAssist((event) => {
      if (event.type === "stateChanged") this.nav.setListening(!["off", "error"].includes(event.state.state));
    });
    this.backend.onFileDrop((kind, paths) => {
      const docx = paths.find((path) => path.toLowerCase().endsWith(".docx"));
      if (document.querySelector("dialog")) return; // the import dialog handles its own drops
      if (kind === "hover") this.landing.highlight(Boolean(docx));
      else if (kind === "leave") this.landing.highlight(false);
      else {
        this.landing.highlight(false);
        if (docx) void this.openImport(docx);
      }
    });
    document.addEventListener("keydown", (event) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        this.filters.focusSearch();
      }
    });
    window.setInterval(() => this.refreshNextUp(), NEXT_UP_REFRESH_MS);
    await this.reload();
  }

  applyTheme(): void {
    const pinned = this.settings.theme;
    const dark = pinned === "dark" || (pinned === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
    document.documentElement.dataset.theme = dark ? "dark" : "light";
    this.rail.refresh();
  }

  async reload(): Promise<void> {
    this.settings = await this.backend.getSettings();
    this.courses = await this.backend.listCourses();
    const chosen = this.chosenGroupId();
    if (chosen !== this.lastChosen) {
      // Another group was chosen (or a new document imported): "another group" no longer applies.
      this.lastChosen = chosen;
      this.viewGroupId = null;
    }
    this.rail.refresh();
    const course = chosen ? this.courseOf(chosen) : undefined;
    const group = course?.groups.find((g) => g.group.id === chosen)?.group;
    if (!group) {
      this.lessons = [];
      this.showLanding(t("landing_title"), t("landing_text"));
      return;
    }
    const shownId = this.shownGroupId() ?? chosen;
    this.lessons = shownId ? await this.backend.groupLessons(shownId) : [];
    if (!this.lessons.length && shownId === chosen) {
      this.showLanding(t("landing_empty_title"), t("landing_empty_text", { group: group.name }));
      return;
    }
    const days = this.visibleDays();
    const occurrences = await this.backend.occurrences(days[0] ?? this.anchor, days[days.length - 1] ?? this.anchor);
    this.statuses = new Map(occurrences.map((o) => [`${o.lesson_id}|${o.date}`, o.status]));
    this.render();
  }

  private showLanding(title: string, text: string): void {
    this.nav.element.hidden = true;
    this.filters.element.hidden = true;
    this.landing.set(title, text);
    replace(this.surface, this.landing.element);
  }

  private render(): void {
    this.nav.element.hidden = false;
    this.filters.element.hidden = false;
    const all = this.lessons;
    const shown = this.shown();
    const course = this.courseOf(this.chosenGroupId() ?? -1);
    this.filters.update(subjects(all), course?.groups.map((g) => g.group) ?? [], this.shownGroupId(), shown.length, all.length);
    this.nav.update();
    this.refreshNextUp();
    const view = this.view();
    replace(this.surface, view === "month" ? renderMonth(this, shown) : renderGrid(this, this.visibleDays(), shown));
  }

  /** The class to outline: today's soonest one that has not been settled. */
  nextKey(lessons: Lesson[]): string | null {
    const today = this.today();
    const now = this.nowMinutes();
    const upcoming = lessons.filter(
      (l) => occursOn(l, today) && toMinutes(l.end) > now && !this.statuses.has(`${l.id}|${today}`),
    );
    if (!upcoming.length) return null;
    const first = upcoming.reduce((a, b) => (toMinutes(b.start) < toMinutes(a.start) ? b : a));
    return `${first.id}|${today}`;
  }

  async refreshNextUp(): Promise<void> {
    const chosen = this.chosenGroupId();
    if (!chosen) return this.nav.setNextUp("", "");
    const lessons = chosen === this.shownGroupId() ? this.lessons : await this.backend.groupLessons(chosen);
    const today = this.today();
    const skipped = new Set(lessons.filter((l) => this.statuses.get(`${l.id}|${today}`) === "skipped").map((l) => l.id));
    const found = nextUp(lessons, today, this.nowMinutes(), skipped);
    if (!found) return this.nav.setNextUp("", "");
    const { lesson } = found;
    const teacher = lesson.teacher.replace(/^\(|\)$/g, "");
    this.nav.setNextUp(nextUpLabel(found), `${lesson.subject}\n${lesson.start}–${lesson.end}${teacher ? `\n${teacher}` : ""}`);
  }

  // -------------------------------------------------------------- navigation

  /** Change what is on screen, morphing from the old picture to the new (PORT_SPEC §F.4). */
  async navigate(change: () => void, morph: "place" | "date" = "place"): Promise<void> {
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const start = (document as Document & { startViewTransition?: (cb: () => Promise<void>) => unknown }).startViewTransition;
    this.morph = morph;
    if (!start || reduce || document.documentElement.classList.contains("still")) {
      change();
      await this.reload();
      return;
    }
    start.call(document, async () => {
      change();
      await this.reload();
    });
  }

  step(direction: 1 | -1): void {
    void this.navigate(() => {
      const view = this.view();
      this.anchor =
        view === "day" ? addDays(this.anchor, direction) : view === "month" ? shiftMonth(this.anchor, direction) : addDays(this.anchor, 7 * direction);
    });
  }

  goToday(): void {
    void this.navigate(() => (this.anchor = this.today()));
  }

  setView(view: ViewMode): void {
    if (view === this.view()) return;
    void this.navigate(() => {
      this.settings.view_mode = view;
      void this.backend.setView(view);
    }, "date");
  }

  openDay(date: IsoDate): void {
    void this.navigate(() => {
      this.anchor = date;
      this.settings.view_mode = "day";
      void this.backend.setView("day");
    }, "date");
  }

  async toggleTheme(): Promise<void> {
    const dark = document.documentElement.dataset.theme === "dark";
    this.settings.theme = dark ? "light" : "dark";
    await this.backend.setTheme(this.settings.theme);
    this.applyTheme();
    await this.reload();
  }

  setFilter(filter: LessonFilter, groupId: number | null): void {
    this.filter = filter;
    const chosen = this.chosenGroupId();
    this.viewGroupId = groupId && groupId !== chosen ? groupId : null;
    void this.reload();
  }

  /** Enter in the search: the nearest date with a match, beyond what is on screen. */
  jumpToMatch(direction: 1 | -1): void {
    if (!isActive(this.filter)) return;
    const days = this.visibleDays();
    const from = direction > 0 ? days[days.length - 1] : days[0];
    const found = from ? findMatch(this.lessons, this.filter, from, direction) : null;
    if (!found) return this.filters.setSummary(t("filter_no_more"));
    void this.navigate(() => (this.anchor = found));
  }

  // --------------------------------------------------------------- actions

  /** Left click: join the class. Without a link, the menu instead — its first item adds one. */
  async joinClass({ lesson, date }: ClassRef, x: number, y: number): Promise<void> {
    if (!lesson.url) return this.classMenu({ lesson, date }, x, y);
    if (!(await this.backend.openLesson(lesson.id, date))) {
      await inform(t("open_failed_title"), t("open_failed_text"));
    }
    await this.reload();
  }

  async classMenu({ lesson, date }: ClassRef, x: number, y: number): Promise<void> {
    const marked = this.statuses.has(`${lesson.id}|${date}`);
    const choice = await showMenu(
      [
        lesson.url ? { id: "open", label: t("menu_open") } : { id: "edit", label: t("menu_add_link") },
        { id: "edit", label: t("menu_edit") },
        "separator",
        { id: "opened", label: t("menu_mark_opened") },
        { id: "skipped", label: t("menu_mark_skipped") },
        ...(marked ? [{ id: "clear", label: t("menu_clear_mark") }] : []),
        "separator",
        ...(date === this.today() ? [{ id: "listen", label: t("assist_listen_this") }] : []),
        { id: "delete", label: t("menu_delete") },
      ],
      x,
      y,
    );
    if (choice === "open") return this.joinClass({ lesson, date }, x, y);
    if (choice === "edit") return this.editClass(lesson, date);
    if (choice === "opened" || choice === "skipped") await this.backend.markLesson(lesson.id, date, choice);
    if (choice === "clear") await this.backend.clearMark(lesson.id, date);
    if (choice === "delete") await this.deleteClass(lesson);
    if (choice === "listen") await this.listenTo(lesson.id);
    await this.reload();
  }

  /** Start the assistant on a class (`null`: the one happening now), asking for what is missing. */
  async listenTo(lessonId: number | null): Promise<void> {
    try {
      await this.backend.assist.assistStart(lessonId);
    } catch (error) {
      await this.assistRefused(String(error), lessonId);
    }
  }

  /** `consent_required`: ask, then try again. `model_missing`: say so and open the settings. */
  private async assistRefused(code: string, lessonId: number | null): Promise<void> {
    if (code === "consent_required") {
      if (await askConsent(this)) await this.listenTo(lessonId);
    } else if (code === "model_missing") {
      await inform(t("assist_title"), t("assist_error_model"));
      await openAssistSettings(this);
    } else {
      await inform(t("assist_title"), t("assist_state_error"));
    }
  }

  private async deleteClass(lesson: Lesson): Promise<void> {
    const r = lesson.recurrence;
    const text =
      r.kind === "weekly_until"
        ? t("delete_series_text", { subject: lesson.subject, date: longDate(r.until).split(" ")[0] ?? "", month: longDate(r.until).split(" ")[1] ?? "" })
        : t("delete_text", { subject: lesson.subject });
    if (await confirmYesNo(t("delete_title"), text)) await this.backend.deleteLesson(lesson.id);
  }

  /** A click on an empty hour: offer to create a class there. */
  async slotClicked(date: IsoDate, hour: number, x: number, y: number): Promise<void> {
    if (!this.chosenGroupId()) return inform(t("import_first_title"), t("import_first_text"));
    const [day, month] = longDate(date).split(" ");
    const time = `${String(hour).padStart(2, "0")}:00`;
    const label = t("grid_create", { day: dayName(weekday(date)), date: day ?? "", month: month ?? "", time });
    const choice = await showMenu([{ id: "create", label }, "separator", { id: "cancel", label: t("menu_cancel") }], x, y);
    if (choice === "create") await this.editClass(null, date, time);
  }

  async editClass(lesson: Lesson | null, date: IsoDate, start?: string): Promise<void> {
    if (!this.chosenGroupId()) return inform(t("import_first_title"), t("import_first_text"));
    if (await openEditor(this, lesson, date, start)) await this.reload();
  }

  /** A class dropped on another date and hour. A repeating one asks what to move. */
  async dropClass({ lesson, date: from }: ClassRef, to: IsoDate, start: string): Promise<void> {
    let scope: "only_this_date" | "whole_series" | null = null;
    if (repeats(lesson)) {
      if (from === to && lesson.start === start) return;
      const [day, month] = longDate(from).split(" ");
      const answer = await ask(t("move_title"), t("move_text", { subject: lesson.subject, date: day ?? "", month: month ?? "" }), [
        { id: "cancel", label: t("cancel") },
        { id: "all", label: t("move_all") },
        { id: "one", label: t("move_one"), primary: true },
      ]);
      if (answer !== "one" && answer !== "all") return;
      scope = answer === "one" ? "only_this_date" : "whole_series";
    }
    await this.backend.moveLesson({ lesson_id: lesson.id, from, to, start, scope });
    await this.reload();
  }

  async offerCatchup(lessonId: number, date: IsoDate, known?: Lesson): Promise<void> {
    const lesson = known ?? (await this.backend.groupLessons(this.chosenGroupId() ?? -1)).find((l) => l.id === lessonId);
    if (lesson) this.catchup.offer(lesson, date);
  }

  async catchupConnect(lesson: Lesson, date: IsoDate): Promise<void> {
    if (!(await this.backend.openLesson(lesson.id, date))) await inform(t("open_failed_title"), t("open_failed_text"));
    await this.reload();
  }

  /** «Закрити»: the class stays missed and does not ask again today. */
  async catchupDismiss(lesson: Lesson, date: IsoDate): Promise<void> {
    await this.backend.markLesson(lesson.id, date, "missed");
    await this.reload();
  }

  async openImport(path?: string): Promise<void> {
    if (await openImport(this, path)) {
      this.viewGroupId = null;
      await this.reload();
    }
  }

  async chooseGroup(): Promise<void> {
    if (await openGroupDialog(this)) await this.reload();
  }

  async openSettings(): Promise<void> {
    if (await openSettings(this)) {
      this.applyTheme();
      await this.reload();
    }
  }

  /** The default length of a new class: from the settings. */
  defaultEnd(start: string): string {
    return addMinutes(start, this.settings.class_duration_minutes);
  }

  monthStart(): IsoDate {
    return firstOfMonth(this.anchor);
  }
}
