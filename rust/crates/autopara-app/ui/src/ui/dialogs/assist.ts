// Lecture Assist: the first-run consent and the assistant's settings (plan step 7.11).
import type { App } from "../../app";
import type { AssistSettings } from "../../bindings/AssistSettings";
import type { Language } from "../../bindings/Language";
import type { ModelStatus } from "../../bindings/ModelStatus";
import { h, replace } from "../../dom";
import { type MessageKey, t } from "../../i18n";
import { ask, confirmYesNo, inform, modal } from "../popups";

const PROVIDERS: [string, MessageKey][] = [
  ["", "assist_llm_none"],
  ["deepseek", "assist_llm_deepseek"],
  ["qwen", "assist_llm_qwen"],
  ["ollama", "assist_llm_ollama"],
  ["lmstudio", "assist_llm_lmstudio"],
  ["llamacpp", "assist_llm_llamacpp"],
  ["custom", "assist_llm_custom"],
];
const LANGUAGES: [Language, MessageKey][] = [
  ["auto", "assist_language_auto"],
  ["uk", "assist_language_uk"],
  ["ru", "assist_language_ru"],
  ["en", "assist_language_en"],
];

const megabytes = (bytes: number) => t("assist_size_mb", { mb: String(Math.round(bytes / 1_000_000)) });
/** Errors come back as string keys; anything else is shown as the generic one. */
const message = (error: unknown) => t((typeof error === "string" && error.startsWith("assist_") ? error : "assist_error_io") as MessageKey);

/** What is captured, where it goes, and whose responsibility it is; true when accepted. */
export async function askConsent(app: App): Promise<boolean> {
  const { retention_days } = await app.backend.assist.assistSettings();
  const answer = await ask(t("assist_consent_title"), t("assist_consent_body", { days: String(retention_days) }), [
    { id: "no", label: t("assist_consent_decline") },
    { id: "yes", label: t("assist_consent_accept"), primary: true },
  ]);
  if (answer !== "yes") return false;
  await app.backend.assist.setAssistConsent(true);
  return true;
}

export async function openAssistSettings(app: App): Promise<void> {
  const assist = app.backend.assist;
  const [s, devices] = await Promise.all([assist.assistSettings(), assist.assistDevices()]);
  const progressOf = new Map<string, HTMLElement>();
  const unlisten = assist.onAssistDownload((id, done, total) => {
    const progress = progressOf.get(id);
    if (progress && total > 0) progress.textContent = t("assist_model_downloading", { percent: String(Math.floor((done / total) * 100)) });
  });
  await modal<boolean>((finish) => {
    const select = (options: [string, string][], value: string, label: MessageKey) =>
      h("select", { class: "select", "aria-label": t(label) }, ...options.map(([v, text]) => h("option", { value: v, selected: v === value }, text)));
    const device = select([["", t("assist_audio_default")], ...devices.map((d): [string, string] => [d.id, d.name])], s.device, "assist_audio_source");
    const language = select(LANGUAGES.map(([v, key]) => [v, t(key)]), s.language, "assist_language");
    const glossary = h("input", { class: "input", type: "text", value: s.glossary, "aria-label": t("assist_glossary") });
    const provider = select(PROVIDERS.map(([v, key]) => [v, t(key)]), s.provider, "assist_llm");
    const baseUrl = h("input", { class: "input", type: "url", value: s.base_url, "aria-label": t("assist_llm_url") });
    const chatModel = h("input", { class: "input", type: "text", value: s.chat_model, "aria-label": t("assist_llm_model") });
    // Write-only: the saved key never comes back to the interface.
    const key = h("input", { class: "input", type: "password", autocomplete: "off", placeholder: t("assist_api_key_new"), "aria-label": t("assist_api_key") });
    let forgetKey = false;
    const keyState = h("span", { class: "hint" }, s.has_key ? t("assist_api_key_saved") : "");
    const forget = h("button", { class: "btn plain", type: "button", hidden: !s.has_key }, t("assist_api_key_forget"));
    forget.addEventListener("click", () => {
      forgetKey = true;
      keyState.textContent = "";
      forget.hidden = true;
    });
    const fallback = h("input", { type: "checkbox", checked: s.fallback });
    const autoListen = h("input", { type: "checkbox", checked: s.auto_listen });
    const retention = h("input", { class: "input", type: "number", min: "1", max: "3650", value: String(s.retention_days), "aria-label": t("assist_retention") });
    const hotkey = h("input", { class: "input", type: "text", value: s.hotkey, "aria-label": t("assist_hotkey") });
    const hotkeyNote = h("span", { class: "hint", role: "status" });
    void assist.assistHotkeyActive().then((active) => (hotkeyNote.textContent = active ? "" : t("assist_hotkey_conflict")));
    const testNote = h("span", { class: "hint", role: "status" });

    let model = s.model;
    const models = h("div", { class: "models" });
    const drawModels = async () => {
      const list = await assist.assistModels();
      replace(models, ...list.filter((m) => !m.model.vad).map(modelRow), ...list.filter((m) => m.model.vad && !m.installed).map(modelRow));
    };
    const modelRow = ({ model: m, installed }: ModelStatus) => {
      const radio = h("input", { type: "radio", name: "model", value: m.id, checked: m.id === model, disabled: !installed || m.vad });
      radio.addEventListener("change", () => (model = m.id));
      const progress = h("span", { class: "hint" });
      const action = installed
        ? h("button", { class: "btn plain", type: "button" }, t("assist_model_delete"))
        : h("button", { class: "btn", type: "button" }, t("assist_model_download", { size: megabytes(m.size) }));
      action.addEventListener("click", async () => {
        action.disabled = true;
        try {
          if (installed) await assist.assistDeleteModel(m.id);
          else await assist.assistDownloadModel(m.id);
        } catch (error) {
          await inform(t("assist_settings"), message(error));
        }
        await drawModels();
      });
      progressOf.set(m.id, progress);
      return h("label", { class: "check" }, radio, `${m.id} · ${megabytes(m.size)}`, h("span", { class: "grow" }), progress, action);
    };
    void drawModels();

    const input = (): AssistSettings => ({
      ...s,
      device: device.value,
      model,
      language: language.value as Language,
      glossary: glossary.value,
      provider: provider.value,
      base_url: baseUrl.value.trim(),
      chat_model: chatModel.value.trim(),
      fallback: fallback.checked,
      auto_listen: autoListen.checked,
      retention_days: Math.min(3650, Math.max(1, Math.round(Number(retention.value)) || 30)),
    });
    const persist = async () => {
      const apiKey = forgetKey ? "" : key.value.trim() || null;
      await assist.saveAssistSettings(input(), apiKey);
      if (hotkey.value.trim() !== s.hotkey) await assist.setAssistHotkey(hotkey.value.trim());
    };
    const save = async () => {
      try {
        await persist();
        finish(true);
      } catch (error) {
        hotkeyNote.textContent = message(error);
      }
    };
    const test = async () => {
      testNote.textContent = "";
      try {
        await persist();
        await assist.assistTestConnection();
        testNote.textContent = t("assist_test_ok");
      } catch (error) {
        testNote.textContent = message(error);
      }
    };
    const revoke = async () => {
      await assist.setAssistConsent(false);
      finish(false);
    };
    const wipe = async () => {
      if (await confirmYesNo(t("assist_delete_all"), t("assist_delete_all_confirm"))) await assist.assistDeleteAll();
    };

    return h(
      "form",
      { class: "dialog settings", onsubmit: (event) => (event.preventDefault(), void save()) },
      h("h2", {}, t("assist_settings")),
      h("div", { class: "form" },
        h("label", {}, t("assist_audio_source")), device,
        h("label", {}, t("assist_language")), language,
        h("label", {}, t("assist_glossary")), glossary),
      h("div", { class: "section" }, t("assist_model")),
      models,
      h("div", { class: "section" }, t("assist_llm")),
      h("div", { class: "form" },
        h("label", {}, t("assist_llm")), provider,
        h("label", {}, t("assist_llm_url")), baseUrl,
        h("label", {}, t("assist_llm_model")), chatModel,
        h("label", {}, t("assist_api_key")), key),
      h("div", { class: "row" }, keyState, forget, h("span", { class: "grow" }), testNote,
        h("button", { class: "btn", type: "button", onclick: () => void test() }, t("assist_test_connection"))),
      h("label", { class: "check" }, fallback, t("assist_fallback")),
      h("div", { class: "section" }, t("assist_title")),
      h("div", { class: "form" },
        h("label", {}, t("assist_hotkey")), hotkey,
        h("label", {}, t("assist_retention")), h("div", { class: "suffix-input" }, retention, h("span", {}, t("assist_retention_suffix")))),
      h("label", { class: "check" }, autoListen, t("assist_auto_listen")),
      h("div", { class: "row" }, hotkeyNote),
      h("div", { class: "row" },
        h("button", { class: "btn plain", type: "button", onclick: () => void wipe() }, t("assist_delete_all")),
        h("span", { class: "grow" }),
        s.consent ? h("button", { class: "btn plain", type: "button", onclick: () => void revoke() }, t("assist_consent_revoke")) : null),
      h("div", { class: "footer" },
        h("button", { class: "btn plain", type: "button", onclick: () => finish(false) }, t("cancel")),
        h("button", { class: "btn primary", type: "submit" }, t("save"))),
    );
  }, t("assist_settings"));
  (await unlisten)();
}
