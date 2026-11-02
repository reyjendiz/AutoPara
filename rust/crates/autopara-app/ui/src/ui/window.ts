// The window's own minimise, maximise and close buttons. On Windows the system title bar is off so
// the calendar can have that row; closing still only hides the window to the tray.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { h } from "../dom";
import { t } from "../i18n";
import { icon, type GlyphName } from "../icons";

export function windowControls(): HTMLElement {
  const win = getCurrentWindow();
  const button = (glyph: GlyphName, tip: string, onclick: () => void, extra = "") =>
    h("button", { class: `icon-btn ${extra}`.trim(), "data-tip": tip, "aria-label": tip, onclick }, icon(glyph, 16));
  const maximize = button("maximize", t("window_maximize"), () => void win.toggleMaximize());
  const refresh = async () => {
    const maximized = await win.isMaximized();
    const tip = t(maximized ? "window_restore" : "window_maximize");
    maximize.replaceChildren(icon(maximized ? "restore" : "maximize", 16));
    maximize.dataset.tip = tip;
    maximize.setAttribute("aria-label", tip);
  };
  void win.onResized(() => void refresh());
  void refresh();
  document.documentElement.classList.add("own-titlebar");
  return h(
    "div",
    { class: "window-controls" },
    button("minimize", t("window_minimize"), () => void win.minimize()),
    maximize,
    button("close", t("window_close"), () => void win.close(), "close"),
  );
}
