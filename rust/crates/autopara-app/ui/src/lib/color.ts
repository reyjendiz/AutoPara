// Subject colours (PORT_SPEC §F.1): the folded subject's MD5 picks one of seven pastels, exactly
// as the Python app does, so a subject keeps its colour across the two apps.

export const SUBJECT_COLORS = ["#ec6b66", "#f1f36a", "#86cdf7", "#8566e6", "#8fe8a8", "#f26c9c", "#a8bdd6"];
export const TINT_LIGHT = 0.16;
export const TINT_DARK = 0.2;

/** MD5 of a byte array, as hex. Small and self-contained: only used to pick a colour. */
export function md5(bytes: Uint8Array): string {
  const shifts = [7, 12, 17, 22, 5, 9, 14, 20, 4, 11, 16, 23, 6, 10, 15, 21];
  const k = Array.from({ length: 64 }, (_, i) => Math.floor(Math.abs(Math.sin(i + 1)) * 2 ** 32) >>> 0);
  const length = bytes.length;
  const padded = new Uint8Array((((length + 8) >> 6) + 1) << 6);
  padded.set(bytes);
  padded[length] = 0x80;
  const view = new DataView(padded.buffer);
  view.setUint32(padded.length - 8, (length * 8) >>> 0, true);
  view.setUint32(padded.length - 4, Math.floor((length * 8) / 2 ** 32), true);
  let [a0, b0, c0, d0] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
  for (let chunk = 0; chunk < padded.length; chunk += 64) {
    let [a, b, c, d] = [a0, b0, c0, d0];
    for (let i = 0; i < 64; i++) {
      let f: number;
      let g: number;
      if (i < 16) [f, g] = [(b & c) | (~b & d), i];
      else if (i < 32) [f, g] = [(d & b) | (~d & c), (5 * i + 1) % 16];
      else if (i < 48) [f, g] = [b ^ c ^ d, (3 * i + 5) % 16];
      else [f, g] = [c ^ (b | ~d), (7 * i) % 16];
      const sum = (a + f + (k[i] ?? 0) + view.getUint32(chunk + g * 4, true)) >>> 0;
      const shift = shifts[(i >> 4) * 4 + (i % 4)] ?? 0;
      [a, d, c] = [d, c, b];
      b = (b + ((sum << shift) | (sum >>> (32 - shift)))) >>> 0;
    }
    [a0, b0, c0, d0] = [(a0 + a) >>> 0, (b0 + b) >>> 0, (c0 + c) >>> 0, (d0 + d) >>> 0];
  }
  return [a0, b0, c0, d0]
    .map((word) => [0, 8, 16, 24].map((s) => ((word >>> s) & 0xff).toString(16).padStart(2, "0")).join(""))
    .join("");
}

/** Python's `subject.strip().casefold()`, for the cases a timetable can contain. */
function caseFold(text: string): string {
  return text.trim().toLowerCase().replace(/ß/g, "ss").replace(/ς/g, "σ");
}

export function subjectColor(subject: string): string {
  const digest = md5(new TextEncoder().encode(caseFold(subject)));
  return SUBJECT_COLORS[parseInt(digest.slice(0, 8), 16) % SUBJECT_COLORS.length] ?? "#a8bdd6";
}

/** `base` with `amount` (0..1) of `colour` mixed in, as an opaque `#rrggbb`. */
export function tint(base: string, colour: string, amount: number): string {
  const rgb = (hex: string) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
  const [under, over] = [rgb(base), rgb(colour)];
  return `#${under
    .map((value, i) => Math.round(value + ((over[i] ?? value) - value) * amount).toString(16).padStart(2, "0"))
    .join("")}`;
}
