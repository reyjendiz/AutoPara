// The strip that shows the assistant's models arriving: AutoPara fetches them by itself once the
// user has agreed to the assistant (assist.rs `prepare`), so the progress is shown unasked.
import type { AssistBackend } from "../backend";
import { h, style } from "../dom";
import { t } from "../i18n";
import { icon } from "../icons";

export function setupBanner(assist: Pick<AssistBackend, "onAssistDownload">): HTMLElement {
  const detail = h("div", { class: "detail" });
  const bar = h("div", {});
  const element = h(
    "section",
    { class: "banner update", role: "status", hidden: true },
    icon("download", 22),
    h("div", { class: "texts" }, h("div", { class: "title" }, t("assist_setup_title")), detail, h("div", { class: "progress" }, bar)),
  );
  void assist.onAssistDownload((_id, done, total) => {
    const percent = total > 0 ? Math.floor((done / total) * 100) : 0;
    element.hidden = done >= total;
    detail.textContent = t("assist_setup_progress", { percent: String(percent) });
    style(bar, { width: `${percent}%` });
  });
  return element;
}
