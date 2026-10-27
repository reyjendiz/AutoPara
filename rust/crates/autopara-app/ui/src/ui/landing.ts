// The first-run screen (Python `ui/import_landing.py`) and the drop well it shares with the import
// dialog. A `.docx` can be dropped anywhere on the window; the well is what lights up.
import type { App } from "../app";
import { h } from "../dom";
import { t } from "../i18n";
import { icon } from "../icons";

export class DropWell {
  readonly element: HTMLElement;
  private readonly file = h("div", { class: "file", hidden: true });
  private readonly prompt = h("div", { class: "prompt" });

  constructor(onClick: () => void, compact = false) {
    this.element = h(
      "div",
      { class: compact ? "well compact" : "well", role: "button", tabindex: "0", onclick: onClick, onkeydown: (e) => {
        if ((e as KeyboardEvent).key === "Enter" || (e as KeyboardEvent).key === " ") onClick();
      } },
      icon("document", compact ? 26 : 34),
      this.file,
      this.prompt,
    );
    this.showPrompt();
  }

  showPrompt(): void {
    this.file.hidden = true;
    this.prompt.textContent = t("drop_prompt");
    this.element.setAttribute("aria-label", t("drop_prompt"));
  }

  /** The chosen file's name, never its path: the path is not what anyone checks. */
  showFile(path: string): void {
    const name = path.split(/[\\/]/).pop() ?? path;
    this.file.textContent = name;
    this.file.hidden = false;
    this.prompt.textContent = t("drop_other");
    this.element.setAttribute("aria-label", `${name}. ${t("drop_other")}`);
  }

  highlight(on: boolean): void {
    this.element.classList.toggle("hover", on);
  }
}

export class Landing {
  readonly element: HTMLElement;
  private readonly title = h("h1", {});
  private readonly text = h("p", {});
  private readonly well: DropWell;

  constructor(app: App) {
    this.well = new DropWell(() => void app.openImport());
    this.element = h(
      "section",
      { class: "landing" },
      h("img", { class: "hero", src: "/logo.png", alt: "" }),
      this.title,
      this.text,
      this.well.element,
      h("button", { class: "btn primary", onclick: () => void app.openImport() }, t("landing_browse")),
    );
  }

  set(title: string, text: string): void {
    this.title.textContent = title;
    this.text.textContent = text;
  }

  highlight(on: boolean): void {
    this.well.highlight(on);
  }
}
