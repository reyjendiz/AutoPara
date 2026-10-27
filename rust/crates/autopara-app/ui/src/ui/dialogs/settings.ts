// Settings (Python `ui/settings_dialog.py`), plus the diagnostics button and the place reserved for
// the lecture assistant.
import type { App } from "../../app";
import type { CatchupMode } from "../../bindings/CatchupMode";
import type { Theme } from "../../bindings/Theme";
import type { UpdateMode } from "../../bindings/UpdateMode";
import { h } from "../../dom";
import { t, type MessageKey } from "../../i18n";
import { inform, modal } from "../popups";

export function openSettings(app: App): Promise<boolean | null> {
  const s = app.settings;
  return modal<boolean>((finish) => {
    const number = (value: number, min: number, max: number, suffix: MessageKey, label: MessageKey) =>
      h("div", { class: "suffix-input" },
        h("input", { class: "input", type: "number", min: String(min), max: String(max), value: String(value), "aria-label": t(label) }),
        h("span", {}, t(suffix)));
    const choices = <T extends string>(options: [T, MessageKey][], value: T, label: MessageKey) =>
      h("select", { class: "select", "aria-label": t(label) }, ...options.map(([v, key]) => h("option", { value: v, selected: v === value }, t(key))));

    const lead = number(s.lead_minutes, 0, 120, "settings_lead_suffix", "settings_lead");
    const duration = number(s.class_duration_minutes, 10, 240, "settings_minutes_suffix", "settings_duration");
    const theme = choices<Theme>([["system", "settings_theme_system"], ["light", "settings_theme_light"], ["dark", "settings_theme_dark"]], s.theme, "settings_theme");
    const remind = h("input", { type: "checkbox", id: "remind", checked: s.notifications_enabled });
    const notify = number(s.notify_minutes, 1, 120, "settings_lead_suffix", "settings_remind_time");
    const notifyInput = notify.querySelector("input") as HTMLInputElement;
    notifyInput.disabled = !s.notifications_enabled;
    remind.addEventListener("change", () => (notifyInput.disabled = !remind.checked));
    const modes: [CatchupMode, MessageKey][] = [["notify", "settings_missed_notify"], ["open", "settings_missed_open"], ["missed", "settings_missed_mark"]];
    const radios = modes.map(([mode, key]) =>
      h("label", { class: "check" }, h("input", { type: "radio", name: "catchup", value: mode, checked: mode === s.catchup_mode }), t(key)));
    const mac = app.backend.isMac();
    const autostart = h("input", { type: "checkbox", id: "autostart", checked: s.autostart_enabled });
    const updates = choices<UpdateMode>([["ask", "settings_update_ask"], ["auto", "settings_update_auto"], ["off", "settings_update_off"]], s.update_mode, "settings_updates");
    const version = h("span", { class: "hint" });
    void app.backend.version().then((v) => (version.textContent = t("settings_version", { version: v })));
    const copy = h("button", { class: "btn", type: "button" }, t("settings_diagnostics"));
    copy.addEventListener("click", async () => {
      await navigator.clipboard.writeText(await app.backend.diagnostics());
      copy.textContent = t("settings_diagnostics_done");
    });

    const value = (holder: HTMLElement) => Number((holder.querySelector("input") as HTMLInputElement).value);
    const save = async () => {
      const clamp = (n: number, min: number, max: number, fallback: number) => (Number.isFinite(n) ? Math.min(max, Math.max(min, Math.round(n))) : fallback);
      const catchup = (radios.map((r) => r.querySelector("input") as HTMLInputElement).find((r) => r.checked)?.value ?? "notify") as CatchupMode;
      try {
        app.settings = await app.backend.saveSettings({
          lead_minutes: clamp(value(lead), 0, 120, 1),
          class_duration_minutes: clamp(value(duration), 10, 240, 80),
          catchup_mode: catchup,
          notifications_enabled: remind.checked,
          notify_minutes: clamp(value(notify), 1, 120, 10),
          theme: theme.value as Theme,
          update_mode: updates.value as UpdateMode,
          autostart_enabled: autostart.checked,
        });
        finish(true);
      } catch {
        await inform(t("settings_autostart_failed_title"), t("settings_autostart_failed"));
      }
    };

    return h(
      "form",
      { class: "dialog settings", onsubmit: (event) => (event.preventDefault(), void save()) },
      h("h2", {}, t("settings_title")),
      h("div", { class: "form" },
        h("label", {}, t("settings_lead")), lead,
        h("label", {}, t("settings_duration")), duration,
        h("label", {}, t("settings_theme")), theme),
      h("div", { class: "section" }, t("settings_notifications")),
      h("label", { class: "check" }, remind, t("settings_remind")),
      h("div", { class: "form" }, h("label", {}, t("settings_remind_time")), notify),
      h("div", { class: "section" }, t("settings_missed")),
      h("div", { role: "radiogroup", "aria-label": t("settings_missed") }, ...radios),
      h("p", { class: "hint" }, t("settings_missed_hint")),
      h("label", { class: "check" }, autostart, t(mac ? "settings_autostart_mac" : "settings_autostart_windows")),
      h("p", { class: "hint" }, t(mac ? "settings_autostart_mac_hint" : "settings_autostart_windows_hint")),
      h("div", { class: "section" }, t("settings_updates")),
      updates,
      h("div", { class: "row" }, version, h("span", { class: "grow" }), h("button", { class: "btn", type: "button", disabled: true }, t("settings_check_now"))),
      h("p", { class: "hint" }, t("settings_updates_hint")),
      h("p", { class: "hint" }, t("settings_duration_hint")),
      h("div", { class: "section" }, t("settings_assist")),
      h("p", { class: "hint" }, t("settings_assist_soon")),
      h("div", { class: "row" }, copy),
      h("div", { class: "footer" },
        h("button", { class: "btn plain", type: "button", onclick: () => finish(false) }, t("cancel")),
        h("button", { class: "btn primary", type: "submit" }, t("save"))),
    );
  }, t("settings_title"));
}
