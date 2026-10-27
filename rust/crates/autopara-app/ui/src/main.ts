import "./styles/tokens.css";
import "./styles/app.css";
import { App } from "./app";
import { demoBackend, tauriBackend } from "./backend";
import { t } from "./i18n";
import { installTooltips } from "./ui/popups";

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
  await app.start();
}

void start();
