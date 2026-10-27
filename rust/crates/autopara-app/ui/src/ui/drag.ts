// Moving a class with the pointer. Not HTML drag and drop: the webview's own file-drop handling
// (which brings in a `.docx` with its real path) disables it on Windows. A press only counts as a
// click when the pointer never travelled far enough to start a drag (Python `ClassCard`).
import type { ClassRef } from "../app";

const DRAG_DISTANCE = 4;

export interface DropTarget {
  date: string;
  /** The start time the drop gives the class; null keeps its own (the month view). */
  start: string | null;
}

/** The drop target under a point: an element carrying `data-drop-date` (and `data-drop-start`). */
function targetAt(x: number, y: number): { element: HTMLElement; target: DropTarget } | null {
  const element = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-drop-date]");
  if (!element?.dataset.dropDate) return null;
  return { element, target: { date: element.dataset.dropDate, start: element.dataset.dropStart ?? null } };
}

/** Make `element` a click target and a drag source for `ref`. */
export function draggable(
  element: HTMLElement,
  ref: ClassRef,
  onClick: (x: number, y: number) => void,
  onDrop: (target: DropTarget) => void,
): void {
  element.addEventListener("pointerdown", (down) => {
    if (down.button !== 0) return;
    const [x0, y0] = [down.clientX, down.clientY];
    let ghost: HTMLElement | null = null;
    let over: HTMLElement | null = null;
    const box = element.getBoundingClientRect();

    const move = (event: PointerEvent) => {
      if (!ghost && Math.abs(event.clientX - x0) + Math.abs(event.clientY - y0) < DRAG_DISTANCE) return;
      if (!ghost) {
        ghost = element.cloneNode(true) as HTMLElement;
        ghost.classList.add("dragging");
        ghost.style.setProperty("position", "fixed");
        ghost.style.setProperty("width", `${box.width}px`);
        ghost.style.setProperty("height", `${box.height}px`);
        ghost.style.removeProperty("view-transition-name");
        document.body.append(ghost);
      }
      ghost.style.setProperty("left", `${event.clientX - (x0 - box.left)}px`);
      ghost.style.setProperty("top", `${event.clientY - (y0 - box.top)}px`);
      const found = targetAt(event.clientX, event.clientY)?.element ?? null;
      if (found !== over) {
        over?.classList.remove("dropping");
        found?.classList.add("dropping");
        over = found;
      }
    };
    const up = (event: PointerEvent) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      over?.classList.remove("dropping");
      if (!ghost) return onClick(event.clientX, event.clientY);
      ghost.remove();
      const found = targetAt(event.clientX, event.clientY);
      if (found) onDrop(found.target);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  });
  void ref;
}
