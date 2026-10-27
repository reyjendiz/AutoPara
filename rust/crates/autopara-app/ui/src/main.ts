import "./styles.css";
import { h } from "./dom";
import { t } from "./i18n";
import { ping } from "./ipc";

async function start(): Promise<void> {
  document.title = t("app_title");
  const version = h("p", { class: "version" });
  document.querySelector("#app")?.append(h("main", {}, h("h1", {}, t("app_title")), version));
  const pong = await ping();
  version.textContent = t("settings_version", { version: pong.version });
}

void start();
