// The strip above the calendar (Python `ui/nav_bar.py`): the period, the "now / next" pill, the
// day / week / month control with its gliding pill, "Сьогодні" and the arrows.
import type { App } from "../app";
import type { ViewMode } from "../bindings/ViewMode";
import { h } from "../dom";
import { t, type MessageKey } from "../i18n";
import { icon } from "../icons";
import { dayTitle, monthTitle, weekStart, weekTitle } from "../lib/calendar";

const VIEWS: [ViewMode, MessageKey][] = [["day", "nav_day"], ["week", "nav_week"], ["month", "nav_month"]];
/** Narrower than this, the bar has no room to say what is next. */
const NEXT_UP_MIN_WIDTH = 1000;

export class NavBar {
  readonly element: HTMLElement;
  private readonly title = h("h1", { class: "nav-title" });
  private readonly sub = h("span", { class: "nav-sub" });
  private readonly nextUp = h("div", { class: "nextup", role: "status" });
  private readonly listen: HTMLButtonElement;
  private listening = false;
  private readonly thumb = h("div", { class: "thumb", "aria-hidden": "true" });
  private readonly segments: Map<ViewMode, HTMLButtonElement>;
  private readonly previous: HTMLButtonElement;
  private readonly next: HTMLButtonElement;

  constructor(private readonly app: App) {
    this.segments = new Map(
      VIEWS.map(([view, key]) => [
        view,
        h("button", { role: "radio", "aria-checked": "false", onclick: () => app.setView(view) }, t(key)),
      ]),
    );
    this.listen = h("button", { class: "btn listen", onclick: () => void (this.listening ? app.backend.assist.assistStop() : app.listenTo(null)) });
    this.setListening(false);
    this.previous = h("button", { class: "icon-btn", onclick: () => app.step(-1) }, icon("chevron_left", 18));
    this.next = h("button", { class: "icon-btn", onclick: () => app.step(1) }, icon("chevron_right", 18));
    this.element = h(
      "header",
      { class: "nav" },
      this.title,
      this.sub,
      this.nextUp,
      this.listen,
      h("div", { class: "segmented", role: "radiogroup" }, this.thumb, ...this.segments.values()),
      h("button", { class: "btn", "data-tip": t("nav_today_tip"), onclick: () => app.goToday() }, t("nav_today")),
      this.previous,
      this.next,
      h("div", { class: "grow" }),
    );
    new ResizeObserver(() => this.placeThumb(false)).observe(this.element);
  }

  update(): void {
    const view = this.app.view();
    const [title, quiet] =
      view === "day" ? dayTitle(this.app.anchor) : view === "month" ? monthTitle(this.app.anchor) : weekTitle(weekStart(this.app.anchor));
    this.title.textContent = title;
    this.sub.textContent = quiet;
    const unit = view === "day" ? "day" : view === "month" ? "month" : "week";
    for (const [button, key] of [
      [this.previous, `nav_previous_${unit}`],
      [this.next, `nav_next_${unit}`],
    ] as [HTMLButtonElement, MessageKey][]) {
      button.dataset.tip = t(key);
      button.setAttribute("aria-label", t(key));
    }
    for (const [mode, button] of this.segments) {
      button.setAttribute("aria-checked", String(mode === view));
    }
    this.placeThumb(true);
  }

  /** The black pill behind the chosen segment: it glides there (220 ms), and its text lights when
   * the pill is nearer the new segment than the old one. */
  private placeThumb(animate: boolean): void {
    const view = this.app.settings?.view_mode ?? "week";
    const button = this.segments.get(view);
    if (!button || !button.offsetWidth) return;
    if (!animate) this.thumb.style.setProperty("transition", "none");
    this.thumb.style.setProperty("left", `${button.offsetLeft}px`);
    this.thumb.style.setProperty("width", `${button.offsetWidth}px`);
    if (!animate) {
      void this.thumb.offsetWidth;
      this.thumb.style.removeProperty("transition");
    }
    window.setTimeout(() => {
      for (const [mode, segment] of this.segments) segment.classList.toggle("on", mode === view);
    }, animate ? 110 : 0);
  }

  setNextUp(text: string, tooltip: string): void {
    this.nextUp.textContent = text;
    this.nextUp.dataset.tip = tooltip;
    this.nextUp.hidden = !text || this.element.offsetWidth < NEXT_UP_MIN_WIDTH;
  }

  /** The listen button beside the pill: a red dot and "stop" while the assistant runs. */
  setListening(on: boolean): void {
    this.listening = on;
    this.listen.classList.toggle("on", on);
    this.listen.textContent = t(on ? "tray_stop_listening" : "tray_listen");
  }
}
