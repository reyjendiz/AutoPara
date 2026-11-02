// The update strip (Python `ui/update_banner.py`): offered → downloading → ready to install. A
// new version is news, not trouble, so the card is neutral. Installing is always a click here.
import type { App } from "../app";
import type { UpdateStatus } from "../bindings/UpdateStatus";
import { h, morph, style } from "../dom";
import { t } from "../i18n";
import { icon } from "../icons";
import { inform } from "./popups";

export class UpdateBanner {
  readonly element: HTMLElement;
  private readonly title = h("div", { class: "title" });
  private readonly detail = h("div", { class: "detail" });
  private readonly bar = h("div", {});
  private readonly progress = h("div", { class: "progress", hidden: true }, this.bar);
  private readonly buttons = h("div", { class: "row" });

  constructor(private readonly app: App) {
    this.element = h(
      "section",
      { class: "banner update", role: "status", hidden: true },
      icon("download", 22),
      h("div", { class: "texts" }, this.title, this.detail, this.progress),
      this.buttons,
    );
  }

  private button(label: string, kind: string, action: () => void): HTMLButtonElement {
    return h("button", { class: `btn ${kind}`, onclick: action }, label);
  }

  async show(status: UpdateStatus): Promise<void> {
    const backend = this.app.backend;
    this.progress.hidden = true;
    switch (status.state) {
      case "available": {
        this.title.textContent = t("update_available", { version: status.version });
        this.detail.textContent = status.notes || t("update_current", { version: await backend.version() });
        const main = status.installable
          ? this.button(t("update_download"), "primary", () => void backend.downloadUpdate())
          : this.button(t("update_open_page"), "primary", () => void backend.openPage(status.page));
        this.buttons.replaceChildren(
          ...(status.installable ? [this.button(t("update_whats_new"), "plain", () => void backend.openPage(status.page))] : []),
          this.button(t("update_skip"), "plain", () => {
            void backend.skipUpdate(status.version);
            this.element.hidden = true;
          }),
          this.button(t("update_later"), "plain", () => (this.element.hidden = true)),
          main,
        );
        break;
      }
      case "downloading":
        this.title.textContent = t("update_downloading", { version: status.version });
        this.detail.textContent = `${status.percent}%`;
        this.progress.hidden = false;
        style(this.bar, { width: `${status.percent}%` });
        this.buttons.replaceChildren();
        break;
      case "ready":
        this.title.textContent = t("update_ready", { version: status.version });
        this.detail.textContent = t("update_ready_restart");
        this.buttons.replaceChildren(
          this.button(t("update_later"), "plain", () => (this.element.hidden = true)),
          this.button(t("update_install"), "primary", () => void backend.installUpdate()),
        );
        break;
      case "up_to_date":
        if (status.manual) await inform(t("update_none_title"), t("update_none_text", { version: status.version }));
        return;
      case "failed":
        if (status.manual) await inform(t("update_failed_title"), t("update_failed_text"));
        return;
    }
    if (this.element.hidden) void morph(() => void (this.element.hidden = false));
  }
}
