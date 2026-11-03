// The Lecture Assist panel (plan step 7.10): what the assistant hears and writes, beside the class
// in the "Відкрита пара" tab. It exists from app start, hidden, so it sees every event, and shows
// itself when listening starts.
import "./panel.css";
import type { AskKind } from "../bindings/AskKind";
import type { AssistEvent } from "../bindings/AssistEvent";
import type { AssistState } from "../bindings/AssistState";
import type { Notes } from "../bindings/Notes";
import type { Term } from "../bindings/Term";
import { type Child, h, morph, replace, style } from "../dom";
import { type MessageKey, t } from "../i18n";
import { icon } from "../icons";
import type { AssistBackend } from "../backend";
import * as ipc from "../ipc";
import { elapsed, tail } from "./text";

const TRANSCRIPT_LINES = 6;

interface Look {
  font: number;
  collapsed: boolean;
}

function loadLook(): Look {
  const fallback: Look = { font: 14, collapsed: false };
  try {
    return { ...fallback, ...(JSON.parse(localStorage.getItem("assist-panel") ?? "{}") as Partial<Look>) };
  } catch {
    return fallback;
  }
}

function saveLook(look: Look): void {
  try {
    localStorage.setItem("assist-panel", JSON.stringify(look));
  } catch {
    // A preference, not data: losing it is fine.
  }
}

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
// The latest teacher's question and the answer streaming in for it (`AskKind` "answer").
let answer = { question: "", text: "", done: true };
let error = "";
let classOver = false;
// The user turned the language model off for this class.
let aiOff = false;

const root = h("aside", { class: "assist", "aria-label": t("assist_title"), hidden: true });
const clock = h("span", { class: "assist-clock" });
// How loud what is captured is, so silence (a muted class, the wrong source) shows at a glance.
const meterBar = h("span", {});
const meter = h("span", { class: "assist-meter", role: "meter", "aria-label": t("assist_level"), "aria-valuemin": 0, "aria-valuemax": 100 }, meterBar);

function applyLook(): void {
  style(root, { "--assist-font": `${look.font}px` });
  saveLook(look);
}

function setCollapsed(collapsed: boolean): void {
  look.collapsed = collapsed;
  applyLook();
  void morph(render);
}

/** Closing is offered once listening has stopped; while it runs the panel only folds. */
function close(): void {
  void morph(() => void (root.hidden = true));
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
  const running = listening || state.state === "paused" || state.state === "starting";
  const title = h("span", { class: "title" }, subject || t("assist_title"));
  return h(
    "header",
    {},
    dot,
    clock,
    meter,
    title,
    button(look.collapsed ? "assist_expand" : "assist_collapse", () => setCollapsed(!look.collapsed)),
    running ? null : h("button", { class: "btn plain", type: "button", title: t("assist_hide"), "aria-label": t("assist_hide"), onclick: close }, icon("close", 14)),
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
    button(aiOff ? "assist_ai_on" : "assist_ai_off", () => {
      aiOff = !aiOff;
      void ipc.assistSetChat(!aiOff);
      render();
    }),
  );
}

function ask(kind: AskKind, text = ""): void {
  reply = "";
  void ipc.assistAsk(kind, text);
  render();
}

function body(): HTMLElement {
  const field = h("input", { class: "input", type: "text", placeholder: t("assist_ask_placeholder"), "aria-label": t("assist_ask_placeholder") });
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
    answer.text
      ? h("section", { class: "answer", "aria-live": "polite" }, h("h2", {}, t("assist_answer")), answer.question ? h("p", { class: "faint" }, answer.question) : null, h("p", {}, answer.text))
      : null,
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
    aiOff
      ? h("p", { class: "faint" }, t("assist_ai_is_off"))
      : h(
          "div",
          { class: "helpers" },
          button("assist_recap", () => ask("recap")),
          button("assist_simplify", () => ask("simplify")),
          button("assist_topic", () => ask("topic")),
        ),
    !aiOff && form,
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
  return h("footer", {}, range("assist_font_size", look.font, 11, 22, (v) => (look.font = v)));
}

function render(): void {
  root.classList.toggle("folded", look.collapsed);
  replace(root, header(), !look.collapsed && controls(), !look.collapsed && body(), !look.collapsed && footer());
  tick();
}

function tick(): void {
  const on = state.state === "listening" || state.state === "paused";
  clock.textContent = on ? elapsed(Date.now() - startedAt) : "";
}

function reset(): void {
  [finals, partial, notes, terms, flagged, reply, error, classOver, aiOff] = [[], "", null, [], [], "", "", false, false];
  startedAt = Date.now();
}

let shown: () => void = () => undefined;

function onEvent(event: AssistEvent): void {
  switch (event.type) {
    case "stateChanged":
      if (event.state.state === "starting") {
        reset();
        root.hidden = false;
        shown();
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
      if (event.kind === "answer") {
        if (answer.done && !event.done) answer = { question: flagged[flagged.length - 1] ?? "", text: "", done: false };
        answer.text += event.text;
        answer.done = event.done;
      } else reply += event.text;
      break;
    case "error":
      error = event.code;
      break;
    case "level":
      style(meterBar, { width: `${Math.round(event.level * 100)}%` });
      meter.setAttribute("aria-valuenow", String(Math.round(event.level * 100)));
      return;
  }
  render();
}

/** The panel, hidden until listening starts; `onShow` runs then, so the class tab can come forward. */
export function assistPanel(assist: Pick<AssistBackend, "onAssist" | "onAssistClassOver">, onShow: () => void): HTMLElement {
  shown = onShow;
  applyLook();
  render();
  setInterval(tick, 1000);
  void assist.onAssist(onEvent);
  void assist.onAssistClassOver(() => {
    classOver = true;
    render();
  });
  return root;
}
