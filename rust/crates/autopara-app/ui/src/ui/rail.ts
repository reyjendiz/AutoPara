// The rail down the left (Python `_build_sidebar`): the app mark, a floating black pill of icon
// buttons and a quiet clock. Nothing on it is written; each button's meaning is its tooltip.
import type { App } from "../app";
import { h } from "../dom";
import { t } from "../i18n";
import { icon, type GlyphName } from "../icons";
import { fromMinutes } from "../lib/calendar";

function tool(glyph: GlyphName, label: string, action: () => void, primary = false): HTMLButtonElement {
  return h("button", { class: primary ? "add" : "tool", "aria-label": label, "data-tip": label, onclick: action }, icon(glyph, primary ? 22 : 20));
}

export class Rail {
  readonly element: HTMLElement;
  private readonly brand: HTMLImageElement;
  private readonly group: HTMLButtonElement;
  private readonly theme: HTMLButtonElement;
  private readonly clock: HTMLElement;

  constructor(private readonly app: App) {
    this.brand = h("img", { class: "brand", src: "/logo.png", alt: "AutoPara" });
    this.group = tool("group", t("rail_group"), () => void app.chooseGroup());
    this.theme = tool("moon", t("rail_theme_to_dark"), () => void app.toggleTheme());
    this.clock = h("div", { class: "clock", "data-tip": t("rail_clock"), role: "timer", "aria-live": "off" });
    this.element = h(
      "nav",
      { class: "rail", "aria-label": "AutoPara" },
      this.brand,
      h("div", { class: "spacer" }),
      h(
        "div",
        { class: "rail-pill" },
        tool("add", t("rail_add"), () => void app.editClass(null, app.anchor), true),
        tool("import", t("rail_import"), () => void app.openImport()),
        this.group,
        tool("settings", t("rail_settings"), () => void app.openSettings()),
        this.theme,
      ),
      this.clock,
    );
    this.tick();
    window.setInterval(() => this.tick(), 1000);
  }

  private tick(): void {
    const now = this.app.now();
    this.clock.textContent = `${fromMinutes(now.getHours() * 60 + now.getMinutes())}:${String(now.getSeconds()).padStart(2, "0")}`;
  }

  /** After a theme change or a reload: the toggle's glyph, the mark's tooltip, the group button. */
  refresh(): void {
    const dark = document.documentElement.dataset.theme === "dark";
    const label = dark ? t("rail_theme_to_light") : t("rail_theme_to_dark");
    this.theme.replaceChildren(icon(dark ? "sun" : "moon", 20));
    this.theme.dataset.tip = label;
    this.theme.setAttribute("aria-label", label);
    const app = this.app;
    if (!app.settings) return;
    this.group.disabled = app.courses.length === 0;
    const chosen = app.settings.selected_group_id;
    const course = chosen ? app.courseOf(chosen) : undefined;
    const group = course?.groups.find((g) => g.group.id === chosen)?.group;
    this.brand.dataset.tip = group
      ? [course?.course.name, group.name, group.specialty, t("rail_lead", { minutes: String(app.settings.lead_minutes) })]
          .filter(Boolean)
          .join("\n")
      : t("rail_no_schedule");
  }
}
