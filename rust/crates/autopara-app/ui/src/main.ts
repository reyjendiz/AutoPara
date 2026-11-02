import "./styles/tokens.css";
import "./styles/app.css";
import { App } from "./app";
import { demoBackend, tauriBackend } from "./backend";
import { t } from "./i18n";
import { occursOn } from "./lib/lessons";
import { installTooltips } from "./ui/popups";
import { windowControls } from "./ui/window";

async function start(): Promise<void> {
  document.title = t("app_title");
  const params = new URLSearchParams(location.search);
  const demo = params.has("demo");
  const root = document.documentElement;
  // Windows is set in Segoe UI; the reference pictures (and `?font=inter`) in Inter.
  if (navigator.userAgent.includes("Windows") && params.get("font") !== "inter") root.classList.add("windows");
  if (params.get("font") === "inter") root.classList.add("pin-inter");
  // A still picture: nothing may be mid-morph.
  if (params.has("still")) root.classList.add("still");
  installTooltips();
  const backend = demo ? await demoBackend(params) : tauriBackend();
  const app = new App(backend, document.querySelector("#app") as HTMLElement);
  // Windows only: the Rust side turns the system title bar off there (macOS keeps its own).
  if (!demo && navigator.userAgent.includes("Windows")) document.body.append(windowControls());
  await app.start();
  // The reference pictures of the two dialogs: the editor on today's first class, and settings.
  const dialog = params.get("dialog");
  if (demo && dialog === "edit") {
    const today = app.today();
    const first = app.lessons.filter((l) => occursOn(l, today)).sort((a, b) => a.start.localeCompare(b.start))[0];
    if (first) void app.editClass(first, today);
  } else if (demo && dialog === "settings") {
    void app.openSettings();
  }
}

void start();
