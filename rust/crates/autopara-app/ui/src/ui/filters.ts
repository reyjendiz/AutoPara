// Search, subject filter and group filter (Python `ui/filter_bar.py`). It owns no data: it reports
// changes and jump requests, and the window tells it what to offer and what was found.
import type { App } from "../app";
import type { Group } from "../bindings/Group";
import { h } from "../dom";
import { t } from "../i18n";
import { icon } from "../icons";

export class FilterBar {
  readonly element: HTMLElement;
  private readonly search: HTMLInputElement;
  private readonly clear: HTMLButtonElement;
  private readonly subject: HTMLSelectElement;
  private readonly group: HTMLSelectElement;
  private readonly summary = h("span", { class: "filter-summary", role: "status" });
  private readonly reset: HTMLButtonElement;

  constructor(private readonly app: App) {
    this.search = h("input", {
      class: "input",
      type: "search",
      placeholder: t("filter_placeholder"),
      "aria-label": t("filter_placeholder"),
      "data-tip": t("filter_tip"),
      oninput: () => this.changed(),
      onkeydown: (event) => this.keys(event as KeyboardEvent),
    });
    this.clear = h("button", { class: "clear", hidden: true, "aria-label": t("filter_clear"), onclick: () => this.clearSearch() }, icon("close", 14));
    this.subject = h("select", { class: "select subject", "aria-label": t("filter_subject_tip"), "data-tip": t("filter_subject_tip"), onchange: () => this.changed() });
    this.group = h("select", { class: "select group", "aria-label": t("filter_group_tip"), "data-tip": t("filter_group_tip"), onchange: () => this.changed() });
    this.reset = h("button", { class: "btn plain", hidden: true, "data-tip": t("filter_reset_tip"), onclick: () => this.resetAll() }, t("filter_reset"));
    this.element = h(
      "div",
      { class: "filters", role: "search" },
      h("div", { class: "search" }, h("span", { class: "lead" }, icon("search", 16)), this.search, this.clear),
      this.subject,
      this.group,
      this.summary,
      this.reset,
    );
  }

  private keys(event: KeyboardEvent): void {
    if (event.key === "Escape" && this.search.value) {
      event.preventDefault();
      this.clearSearch();
    } else if (event.key === "Enter") {
      event.preventDefault();
      this.app.jumpToMatch(event.shiftKey ? -1 : 1);
    }
  }

  private clearSearch(): void {
    this.search.value = "";
    this.changed();
  }

  private resetAll(): void {
    this.search.value = "";
    this.subject.value = "";
    this.changed();
  }

  private changed(): void {
    this.clear.hidden = !this.search.value;
    const groupId = this.group.value ? Number(this.group.value) : null;
    this.app.setFilter({ text: this.search.value, subject: this.subject.value }, groupId);
  }

  focusSearch(): void {
    this.search.focus();
    this.search.select();
  }

  /** What to offer and what was found; the chosen subject survives if it is still offered. */
  update(subjects: string[], groups: Group[], shownGroup: number | null, found: number, total: number): void {
    const chosen = this.subject.value;
    this.subject.replaceChildren(
      h("option", { value: "" }, t("filter_all_subjects")),
      ...subjects.map((name) => h("option", { value: name, selected: name === chosen }, name)),
    );
    if (!subjects.includes(chosen)) this.subject.value = "";
    this.group.replaceChildren(...groups.map((g) => h("option", { value: String(g.id), selected: g.id === shownGroup }, g.name)));
    this.group.hidden = groups.length < 2;
    const active = this.search.value.trim() !== "" || this.subject.value !== "";
    this.reset.hidden = !active;
    this.setSummary(!active ? "" : found ? t("filter_found", { found: String(found), total: String(total) }) : t("filter_nothing"));
  }

  setSummary(text: string): void {
    this.summary.textContent = text;
    this.summary.hidden = !text;
  }
}
