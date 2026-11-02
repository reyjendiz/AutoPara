// The two tabs at the top ("Розклад", "Відкрита пара") and the class tab's panel. The class page
// itself is a separate webview the Rust side lays over `view` (src-tauri/src/meeting.rs); this side
// only says where it goes and when it may show.
import type { App } from "../app";
import { h } from "../dom";
import { t } from "../i18n";

function siteName(site: string): string {
  if (site === "meet.google.com") return t("meeting_site_meet");
  if (site === "zoom.us" || site.endsWith(".zoom.us")) return t("meeting_site_zoom");
  return site;
}

export class MeetingTab {
  readonly tabs: HTMLElement;
  readonly panel: HTMLElement;
  private readonly scheduleTab: HTMLButtonElement;
  private readonly meetingTab: HTMLButtonElement;
  private readonly title: HTMLElement;
  private readonly view: HTMLElement;
  private readonly openOnly: HTMLElement[];
  private readonly listen: HTMLButtonElement;
  private listening = false;
  private site: string | null = null;
  private active = false;

  constructor(private readonly app: App) {
    const meeting = app.backend.meeting;
    const tab = (label: string, onclick: () => void) => h("button", { class: "tab", role: "tab", onclick }, label);
    const button = (label: string, onclick: () => void) => h("button", { class: "btn", onclick }, label);
    this.scheduleTab = tab(t("tab_schedule"), () => this.choose(false));
    this.meetingTab = tab(t("tab_meeting"), () => this.choose(true));
    // The row is also the title bar: its empty part drags the window.
    this.tabs = h("div", { class: "tabs", role: "tablist", "data-tauri-drag-region": "" }, this.scheduleTab, this.meetingTab);
    this.title = h("span", { class: "meeting-title" });
    // The same switch as in the schedule's row, so listening starts without leaving the class.
    this.listen = h("button", { class: "btn listen", onclick: () => void (this.listening ? app.backend.assist.assistStop() : app.listenTo(null)) });
    this.setListening(false);
    this.openOnly = [
      button(t("meeting_in_browser"), () => void meeting.inBrowser().then(() => this.closed())),
      button(t("meeting_close"), () => void meeting.close().then(() => this.closed())),
    ];
    this.view = h("div", { class: "meeting-view" }, h("p", {}, t("meeting_empty")));
    this.panel = h(
      "section",
      { class: "meeting", role: "tabpanel" },
      h(
        "div",
        { class: "meeting-bar", "data-tauri-drag-region": "" },
        this.title,
        this.listen,
        h("div", { class: "grow", "data-tauri-drag-region": "" }),
        button(t("meeting_sign_in_google"), () => void meeting.signIn("google")),
        button(t("meeting_sign_in_zoom"), () => void meeting.signIn("zoom")),
        ...this.openOnly,
      ),
      this.view,
    );
    meeting.onOpened((site) => {
      this.site = site;
      this.choose(true);
    });
    new ResizeObserver(() => this.place()).observe(this.view);
    // The page covers whatever is drawn under it, so it steps aside while a dialog is open.
    new MutationObserver(() => this.place()).observe(document.body, { childList: true });
    new MutationObserver(() => this.place()).observe(document.documentElement, { attributeFilter: ["data-theme"] });
    this.render();
  }

  /** Mirrors the schedule's listen button: a red dot and "stop" while the assistant runs. */
  setListening(on: boolean): void {
    this.listening = on;
    this.listen.classList.toggle("on", on);
    this.listen.textContent = t(on ? "tray_stop_listening" : "tray_listen");
  }

  choose(active: boolean): void {
    this.active = active;
    this.scheduleTab.setAttribute("aria-selected", String(!active));
    this.meetingTab.setAttribute("aria-selected", String(active));
    this.panel.parentElement?.classList.toggle("meeting-on", active);
    this.render();
  }

  private closed(): void {
    this.site = null;
    this.render();
  }

  private render(): void {
    const open = this.site !== null;
    this.meetingTab.classList.toggle("live", open);
    this.title.textContent = this.site === null ? t("meeting_none") : siteName(this.site);
    for (const element of this.openOnly) element.style.setProperty("display", open ? "" : "none");
    this.place();
  }

  private place(): void {
    const box = this.view.getBoundingClientRect();
    const visible = this.active && this.site !== null && !document.querySelector("body > dialog");
    // Inside the panel's 1px border; the page's corners are painted in these colours (meeting.rs).
    const border = getComputedStyle(this.view).borderTopColor;
    const background = getComputedStyle(document.body).backgroundColor;
    void this.app.backend.meeting.place(box.x + 1, box.y + 1, box.width - 2, box.height - 2, visible, border, background);
  }
}
