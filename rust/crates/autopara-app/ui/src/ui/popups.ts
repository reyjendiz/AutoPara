// Tooltips, context menus and message boxes — drawn by the page, rounded, in the theme's colours
// (Python `ui/popups.py`). Each returns when the user has answered; nothing here acts by itself.
import { h } from "../dom";
import { t } from "../i18n";

// ---------------------------------------------------------------------- tooltips

const TIP_DELAY_MS = 600;
const TIP_LIFETIME_MS = 12_000;
const TIP_OFFSET = { x: 14, y: 20 };

let tip: HTMLElement | null = null;
let timer = 0;

function hideTip(): void {
  window.clearTimeout(timer);
  tip?.remove();
  tip = null;
}

/** Any element with `data-tip` shows that text near the pointer after a short rest. */
export function installTooltips(): void {
  document.addEventListener("pointerover", (event) => {
    const target = (event.target as Element | null)?.closest<HTMLElement>("[data-tip]");
    if (!target) return;
    const text = target.dataset.tip ?? "";
    const show = () => {
      hideTip();
      if (!text || !target.isConnected) return;
      tip = h("div", { class: "tip", role: "tooltip" }, text);
      document.body.append(tip);
      const { innerWidth, innerHeight } = window;
      const box = tip.getBoundingClientRect();
      let x = Math.min(event.clientX + TIP_OFFSET.x, innerWidth - box.width - 4);
      let y = event.clientY + TIP_OFFSET.y;
      if (y + box.height > innerHeight) y = event.clientY - box.height - 8; // flip above the pointer
      x = Math.max(4, x);
      tip.style.setProperty("left", `${x}px`);
      tip.style.setProperty("top", `${y}px`);
      timer = window.setTimeout(hideTip, TIP_LIFETIME_MS);
    };
    window.clearTimeout(timer);
    timer = window.setTimeout(show, tip ? 0 : TIP_DELAY_MS);
  });
  for (const kind of ["pointerout", "pointerdown", "wheel", "keydown", "blur"]) {
    window.addEventListener(kind, hideTip, { capture: true, passive: true });
  }
}

// ------------------------------------------------------------------------- menus

export type MenuItem = { id: string; label: string } | "separator";

/** A context menu at (x, y). Resolves with the chosen id, or null when dismissed. */
export function showMenu(items: MenuItem[], x: number, y: number): Promise<string | null> {
  hideTip();
  return new Promise((resolve) => {
    const menu = h("div", { class: "menu", role: "menu" });
    const close = (id: string | null) => {
      menu.remove();
      document.removeEventListener("pointerdown", outside, true);
      document.removeEventListener("keydown", keys, true);
      resolve(id);
    };
    const outside = (event: Event) => {
      if (!menu.contains(event.target as Node)) close(null);
    };
    const buttons: HTMLButtonElement[] = [];
    const keys = (event: KeyboardEvent) => {
      const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
      if (event.key === "Escape") close(null);
      else if (event.key === "ArrowDown") buttons[(index + 1) % buttons.length]?.focus();
      else if (event.key === "ArrowUp") buttons[(index - 1 + buttons.length) % buttons.length]?.focus();
      else return;
      event.preventDefault();
    };
    for (const item of items) {
      if (item === "separator") {
        menu.append(h("hr", { role: "separator" }));
        continue;
      }
      const button = h("button", { role: "menuitem", onclick: () => close(item.id) }, item.label);
      buttons.push(button);
      menu.append(button);
    }
    document.body.append(menu);
    const box = menu.getBoundingClientRect();
    const left = Math.min(x, window.innerWidth - box.width - 4);
    const top = y + box.height > window.innerHeight ? Math.max(4, y - box.height) : y;
    menu.style.setProperty("left", `${left}px`);
    menu.style.setProperty("top", `${top}px`);
    menu.style.setProperty("--origin", `${x - left}px ${y - top}px`);
    buttons[0]?.focus();
    window.setTimeout(() => {
      document.addEventListener("pointerdown", outside, true);
      document.addEventListener("keydown", keys, true);
    });
  });
}

// --------------------------------------------------------------------- dialogs

/** Open `content` as a modal dialog; resolves with whatever `finish` was called with, or null on
 * Esc. The dialog is removed when it closes. */
export function modal<T>(build: (finish: (value: T | null) => void) => HTMLElement, label: string): Promise<T | null> {
  hideTip();
  // No morph here: the schedule's named cards would be drawn over the dialog while it grows; the
  // dialog fades in by CSS instead.
  return new Promise((resolve) => {
    const dialog = h("dialog", { "aria-label": label });
    let result: T | null = null;
    const finish = (value: T | null) => {
      result = value;
      dialog.close();
    };
    dialog.append(build(finish));
    dialog.addEventListener("close", () => {
      dialog.remove();
      resolve(result);
    });
    document.body.append(dialog);
    dialog.showModal();
  });
}

export interface Choice {
  id: string;
  label: string;
  primary?: boolean;
}

/** A message with buttons; resolves with the id pressed, null on Esc. */
export function ask(title: string, text: string, choices: Choice[]): Promise<string | null> {
  return modal<string>(
    (finish) =>
      h(
        "div",
        { class: "dialog" },
        h("h2", {}, title),
        h("p", { class: "hint" }, text),
        h(
          "div",
          { class: "footer" },
          ...choices.map((choice) =>
            h(
              "button",
              { class: choice.primary ? "btn primary" : "btn plain", autofocus: Boolean(choice.primary), onclick: () => finish(choice.id) },
              choice.label,
            ),
          ),
        ),
      ),
    title,
  );
}

export async function inform(title: string, text: string): Promise<void> {
  await ask(title, text, [{ id: "ok", label: t("ok"), primary: true }]);
}

export async function confirmYesNo(title: string, text: string): Promise<boolean> {
  return (await ask(title, text, [
    { id: "no", label: t("no") },
    { id: "yes", label: t("yes"), primary: true },
  ])) === "yes";
}
