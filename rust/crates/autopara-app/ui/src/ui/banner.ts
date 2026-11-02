// The missed-class banner (Python `ui/catchup_banner.py`): a class that started while AutoPara was
// not watching is named, with two buttons, and nothing opens until one is pressed. Several queue
// and are shown one at a time.
import type { App } from "../app";
import type { Lesson } from "../bindings/Lesson";
import { h, morph } from "../dom";
import { t } from "../i18n";
import { icon } from "../icons";
import type { IsoDate } from "../lib/calendar";

export class CatchupBanner {
  readonly element: HTMLElement;
  private readonly title = h("div", { class: "title" });
  private readonly detail = h("div", { class: "detail" });
  private readonly connect: HTMLButtonElement;
  private queue: { lesson: Lesson; date: IsoDate }[] = [];

  constructor(private readonly app: App) {
    this.connect = h("button", { class: "btn primary", onclick: () => this.answer(true) }, t("catchup_connect"));
    this.element = h(
      "section",
      { class: "banner catchup", role: "alert", hidden: true },
      icon("warning", 22),
      h("div", { class: "texts" }, this.title, this.detail),
      this.connect,
      h("button", { class: "btn plain", onclick: () => this.answer(false) }, t("catchup_close")),
    );
  }

  offer(lesson: Lesson, date: IsoDate): void {
    if (this.queue.some((item) => item.lesson.id === lesson.id && item.date === date)) return;
    this.queue.push({ lesson, date });
    this.render();
    this.connect.focus();
  }

  /** Handled elsewhere (opened by the scheduler, or from the card): drop it silently. */
  resolve(lessonId: number): void {
    this.queue = this.queue.filter((item) => item.lesson.id !== lessonId);
    this.render();
  }

  private answer(connect: boolean): void {
    const current = this.queue.shift();
    this.render();
    if (!current) return;
    void (connect ? this.app.catchupConnect(current.lesson, current.date) : this.app.catchupDismiss(current.lesson, current.date));
  }

  private render(): void {
    const current = this.queue[0];
    if (this.element.hidden === !current) this.draw(current);
    else void morph(() => this.draw(current));
  }

  private draw(current: { lesson: Lesson; date: IsoDate } | undefined): void {
    this.element.hidden = !current;
    if (!current) return;
    const { lesson } = current;
    this.title.textContent = t("catchup_title", { subject: lesson.subject });
    const parts = [t("catchup_detail", { start: lesson.start, end: lesson.end })];
    const teacher = lesson.teacher.replace(/^\(|\)$/g, "");
    if (teacher) parts.push(teacher);
    if (this.queue.length > 1) parts.push(t("catchup_queue", { count: String(this.queue.length - 1) }));
    this.detail.textContent = parts.join("  ·  ");
    this.connect.disabled = !lesson.url;
    this.connect.dataset.tip = lesson.url ?? t("catchup_no_link");
  }
}
