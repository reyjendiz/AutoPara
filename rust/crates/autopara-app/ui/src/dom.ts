// The component helpers: build an element with attributes, listeners and children in one call.

export type Child = Node | string | null | undefined | false;

type Attributes = Record<string, string | number | boolean | null | undefined | ((event: Event) => void)>;

/** `h("button", { class: "btn", onclick: go, "aria-label": "…" }, "text")`. Attributes that are
 * functions become listeners (`onclick` → `click`); `false`/`null` attributes are left out. */
export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attributes: Attributes = {},
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const element = document.createElement(tag);
  for (const [name, value] of Object.entries(attributes)) {
    if (typeof value === "function") element.addEventListener(name.slice(2), value);
    else if (value === true) element.setAttribute(name, "");
    else if (value !== false && value !== null && value !== undefined) element.setAttribute(name, String(value));
  }
  for (const child of children) {
    if (child) element.append(child);
  }
  return element;
}

/** Set CSS custom properties or style properties through the CSSOM (the CSP forbids inline
 * `style` attributes, not this). */
export function style(element: HTMLElement, properties: Record<string, string>): HTMLElement {
  for (const [name, value] of Object.entries(properties)) element.style.setProperty(name, value);
  return element;
}

/** An element's content replaced by `children`. */
export function replace(element: Element, ...children: Child[]): void {
  element.replaceChildren(...children.filter((child): child is Node | string => Boolean(child)));
}
