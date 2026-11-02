// Every interface glyph, drawn on the Python app's 24-unit grid (`ui/icons.py`) and coloured by
// `currentColor`, so a theme change needs no repainting.

const stroke = (width: number, body: string) =>
  `<g fill="none" stroke="currentColor" stroke-width="${width}" stroke-linecap="round" stroke-linejoin="round">${body}</g>`;

const line = (x1: number, y1: number, x2: number, y2: number) => `<line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}"/>`;
const poly = (...points: [number, number][]) => `<polyline points="${points.map((p) => p.join(",")).join(" ")}"/>`;
const circle = (cx: number, cy: number, r: number) => `<circle cx="${cx}" cy="${cy}" r="${r}"/>`;

/** The cog: one closed outline of eight teeth, stroked like the icons beside it. */
function cog(): string {
  const [teeth, outer, root] = [8, 10, 7.4];
  const step = (Math.PI * 2) / teeth;
  const [tipHalf, rootHalf] = [step * 0.18, step * 0.3];
  const points: string[] = [];
  for (let i = 0; i < teeth; i++) {
    const centre = i * step;
    for (const [angle, radius] of [
      [centre - tipHalf, outer],
      [centre + tipHalf, outer],
      [centre + rootHalf, root],
      [centre + step - rootHalf, root],
    ] as [number, number][]) {
      points.push(`${(12 + Math.cos(angle) * radius).toFixed(2)},${(12 + Math.sin(angle) * radius).toFixed(2)}`);
    }
  }
  return `<polygon points="${points.join(" ")}"/>`;
}

function sunRays(): string {
  return Array.from({ length: 8 }, (_, i) => {
    const angle = (i * Math.PI) / 4;
    const [c, s] = [Math.cos(angle), Math.sin(angle)];
    return line(12 + c * 7, 12 + s * 7, 12 + c * 9.6, 12 + s * 9.6);
  }).join("");
}

const GLYPHS = {
  add: stroke(2, line(12, 5.5, 12, 18.5) + line(5.5, 12, 18.5, 12)),
  import: stroke(1.9, line(12, 3.6, 12, 14.6) + poly([7.8, 7.8], [12, 3.6], [16.2, 7.8]) + poly([5, 15.5], [5, 20], [19, 20], [19, 15.5])),
  download: stroke(1.9, line(12, 3.6, 12, 14.6) + poly([7.8, 10.4], [12, 14.6], [16.2, 10.4]) + poly([5, 15.5], [5, 20], [19, 20], [19, 15.5])),
  settings: stroke(1.7, cog() + circle(12, 12, 3.5)),
  sun: `<circle cx="12" cy="12" r="4" fill="currentColor"/>${stroke(1.9, circle(12, 12, 4) + sunRays())}`,
  // Disc r 8.6 at (12, 12) minus disc r 8 at (16.4, 8.6); the arcs meet at the circles' crossings.
  moon: `<path fill="currentColor" d="M10.15 3.6A8.6 8.6 0 1 0 19.66 15.9A8 8 0 0 1 10.15 3.6Z"/>`,
  chevron_left: stroke(2, poly([14.5, 5.5], [8, 12], [14.5, 18.5])),
  chevron_right: stroke(2, poly([9.5, 5.5], [16, 12], [9.5, 18.5])),
  group: stroke(
    1.8,
    circle(9, 8.4, 3.3) +
      `<path d="M3.2 19.6C3.2 15.6 6 14.2 9 14.2C12 14.2 14.8 15.6 14.8 19.6"/>` +
      circle(16.6, 9.4, 2.6) +
      `<path d="M17.2 14.3C19.6 14.5 20.8 16.2 20.8 19.6"/>`,
  ),
  document: stroke(1.7, poly([14, 3], [6, 3], [6, 21], [18, 21], [18, 7], [14, 3], [14, 7], [18, 7])),
  warning: stroke(1.9, poly([12, 4], [21, 20], [3, 20], [12, 4]) + line(12, 10, 12, 14.5) + line(12, 17.4, 12, 17.4)),
  search: stroke(1.9, circle(10.6, 10.6, 5.6) + line(14.8, 14.8, 19.4, 19.4)),
  close: stroke(1.9, line(7, 7, 17, 17) + line(17, 7, 7, 17)),
  minimize: stroke(1.7, line(6, 12, 18, 12)),
  maximize: stroke(1.7, poly([6.5, 6.5], [17.5, 6.5], [17.5, 17.5], [6.5, 17.5], [6.5, 6.5])),
  restore: stroke(1.7, poly([6, 9], [15, 9], [15, 18], [6, 18], [6, 9]) + poly([9, 9], [9, 6], [18, 6], [18, 15], [15, 15])),
} as const;

export type GlyphName = keyof typeof GLYPHS;

/** An inline SVG element for `name` at `size` px, hidden from screen readers (the button that
 * holds it carries the label). */
export function icon(name: GlyphName, size = 20): SVGSVGElement {
  const template = document.createElement("template");
  template.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="${size}" height="${size}" aria-hidden="true" focusable="false">${GLYPHS[name]}</svg>`;
  return template.content.firstElementChild as SVGSVGElement;
}
