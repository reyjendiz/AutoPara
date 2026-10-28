// The Lecture Assist overlay (plan step 7.10): a small always-on-top window on the user's own
// screen that shows what the assistant hears and writes. It lives from app start, hidden, so it
// sees every event; the Rust side shows it when listening starts.
import "./overlay.css";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AskKind } from "../bindings/AskKind";
import type { AssistEvent } from "../bindings/AssistEvent";
import type { AssistState } from "../bindings/AssistState";
import type { Notes } from "../bindings/Notes";
import type { Term } from "../bindings/Term";
import { type Child, h, replace, style } from "../dom";
import { type MessageKey, t } from "../i18n";
import { icon } from "../icons";
import * as ipc from "../ipc";
import { elapsed, tail } from "./text";

const TRANSCRIPT_LINES = 6;
const PILL = new LogicalSize(260, 48);

interface Look {
  opacity: number;
  font: number;
  collapsed: boolean;
  width: number;
  height: number;
}

function loadLook(): Look {
  const fallback: Look = { opacity: 85, font: 14, collapsed: false, width: 380, height: 520 };
  try {
    return { ...fallback, ...(JSON.parse(localStorage.getItem("overlay") ?? "{}") as Partial<Look>) };
  } catch {
    return fallback;
  }
}

function saveLook(look: Look): void {
  try {
    localStorage.setItem("overlay", JSON.stringify(look));
  } catch {
    // A preference, not data: losing it is fine.
  }
}

const win = getCurrentWindow();
const look = loadLook();
let state: AssistState = { state: "off" };
let startedAt = 0;
let subject = "";
let finals: string[] = [];
let partial = "";
let notes: Notes | null = null;
let terms: Term[] = [];
let flagged: string[] = [];
let reply = "";
let error = "";
let classOver = false;
let clickThrough = false;

const root = document.querySelector("#overlay") as HTMLElement;
const clock = h("span", { class: "clock" });

function applyLook(): void {
  style(document.documentElement, { "--alpha": String(look.opacity / 100), "--font": `${look.font}px` });
  saveLook(look);
}

async function setCollapsed(collapsed: boolean): Promise<void> {
  if (collapsed && !look.collapsed) {
    const size = (await win.innerSize()).toLogical(await win.scaleFactor());
    [look.width, look.height] = [size.width, size.height];
  }
  look.collapsed = collapsed;
  applyLook();
  await win.setSize(collapsed ? PILL : new LogicalSize(look.width, look.height));
  render();
}

async function setClickThrough(on: boolean): Promise<void> {
  clickThrough = on;
  await win.setIgnoreCursorEvents(on);
  render();
}

function button(key: MessageKey, onclick: () => void): HTMLButtonElement {
  return h("button", { class: "btn", type: "button", onclick }, t(key));
}

function section(key: MessageKey, items: string[]): Child {
  if (items.length === 0) return null;
  return h("section", {}, h("h2", {}, t(key)), h("ul", {}, ...items.map((item) => h("li", {}, item))));
}

function header(): HTMLElement {
  const listening = state.state === "listening";
  const dot = h("span", { class: `dot${listening ? " live" : ""}`, role: "img", "aria-label": t("assist_recording") });
  const title = h("span", { class: "title", "data-tauri-drag-region": true }, subject || t("assist_title"));
  return h(
    "header",
    { "data-tauri-drag-region": true },
    dot,
    clock,
    title,
    button(look.collapsed ? "assist_expand" : "assist_collapse", () => void setCollapsed(!look.collapsed)),
    h("button", { class: "btn", type: "button", title: t("assist_hide"), "aria-label": t("assist_hide"), onclick: () => void win.hide() }, icon("close", 14)),
  );
}

function controls(): Child {
  const listening = state.state === "listening";
  const paused = state.state === "paused";
  if (!listening && !paused) return null;
  return h(
    "div",
    { class: "controls" },
    listening ? button("assist_pause", () => void ipc.assistPause()) : button("assist_resume", () => void ipc.assistResume()),
    button("assist_stop", () => void ipc.assistStop()),
  );
}

function ask(kind: AskKind, text = ""): void {
  reply = "";
  void ipc.assistAsk(kind, text);
  render();
}

function body(): HTMLElement {
  const field = h("input", { type: "text", placeholder: t("assist_ask_placeholder"), "aria-label": t("assist_ask_placeholder") });
  const form = h(
    "form",
    {
      class: "ask",
      onsubmit: (event: Event) => {
        event.preventDefault();
        if (field.value.trim()) ask("free", field.value.trim());
        field.value = "";
      },
    },
    field,
    h("button", { class: "btn", type: "submit" }, t("assist_send")),
  );
  const status: Child =
    (state.state === "error" && h("p", { class: "error", role: "alert" }, t(state.code as MessageKey))) ||
    (error && h("p", { class: "error", role: "alert" }, t(error as MessageKey))) ||
    (classOver &&
      h(
        "p",
        { class: "prompt" },
        t("assist_class_over_title"),
        button("assist_keep_listening", () => {
          classOver = false;
          void ipc.assistKeepListening();
          render();
        }),
        button("assist_stop_listening", () => void ipc.assistStop()),
      ));
  const transcript = [...finals, partial].filter(Boolean);
  return h(
    "main",
    {},
    status,
    h(
      "section",
      { class: "transcript", "aria-live": "polite" },
      h("h2", {}, t("assist_transcript")),
      ...(transcript.length
        ? transcript.map((line, i) => h("p", { class: i === finals.length ? "partial" : "" }, line))
        : [h("p", { class: "faint" }, t("assist_waiting"))]),
    ),
    notes?.topic ? h("p", { class: "topic" }, notes.topic) : null,
    section("assist_notes", [...(notes?.key_points ?? []), ...(notes?.definitions ?? []), ...(notes?.deadlines ?? [])]),
    section("assist_terms", terms.map((term) => `${term.term} — ${term.explanation}`)),
    section("assist_flagged", [...flagged, ...(notes?.flagged ?? [])]),
    reply ? h("section", { class: "reply" }, reply) : null,
    h(
      "div",
      { class: "helpers" },
      button("assist_recap", () => ask("recap")),
      button("assist_simplify", () => ask("simplify")),
      button("assist_topic", () => ask("topic")),
    ),
    form,
  );
}

function range(key: MessageKey, value: number, min: number, max: number, set: (v: number) => void): HTMLElement {
  const input = h("input", { type: "range", min, max, value, "aria-label": t(key) });
  input.addEventListener("input", () => {
    set(Number(input.value));
    applyLook();
  });
  return h("label", {}, t(key), input);
}

function footer(): HTMLElement {
  const through = h("input", { type: "checkbox", checked: clickThrough });
  through.addEventListener("change", () => void setClickThrough(through.checked));
  return h(
    "footer",
    {},
    range("assist_opacity", look.opacity, 40, 100, (v) => (look.opacity = v)),
    range("assist_font_size", look.font, 11, 22, (v) => (look.font = v)),
    h("label", { title: t("assist_click_through_hint") }, through, t("assist_click_through")),
  );
}

function render(): void {
  root.className = look.collapsed ? "pill" : "panel";
  replace(root, header(), !look.collapsed && controls(), !look.collapsed && body(), !look.collapsed && footer());
  tick();
}

function tick(): void {
  const on = state.state === "listening" || state.state === "paused";
  clock.textContent = on ? elapsed(Date.now() - startedAt) : "";
}

function reset(): void {
  [finals, partial, notes, terms, flagged, reply, error, classOver] = [[], "", null, [], [], "", "", false];
  startedAt = Date.now();
}

function onEvent(event: AssistEvent): void {
  switch (event.type) {
    case "stateChanged":
      if (event.state.state === "starting") {
        reset();
        // Not awaited: later events must not wait behind it, or the state would go backwards.
        void ipc.assistSubject().then((title) => {
          subject = title ?? "";
          render();
        });
      }
      state = event.state;
      break;
    case "transcriptPartial":
      partial = event.text;
      break;
    case "transcriptFinal":
      finals = tail(finals, event.segment.text, TRANSCRIPT_LINES);
      partial = "";
      break;
    case "notesUpdated":
      notes = event.notes;
      break;
    case "termsUpdated":
      terms = event.terms;
      break;
    case "flagged":
      flagged = [...flagged, event.segment.text];
      break;
    case "helperReply":
      reply += event.text;
      break;
    case "error":
      error = event.code;
      break;
    case "level":
      return;
  }
  render();
}

applyLook();
render();
setInterval(tick, 1000);
void ipc.onAssist(onEvent);
void ipc.onAssistClassOver(() => {
  classOver = true;
  render();
});
// Opening AutoPara from the tray turns click-through off (the Rust side has already done it).
void listen("overlay-solid", () => {
  clickThrough = false;
  render();
});
