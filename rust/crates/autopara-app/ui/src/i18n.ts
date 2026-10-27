// The interface's strings come from the same table the Rust side reads (autopara-core/i18n).
import uk from "../../../autopara-core/i18n/uk.json";

export type MessageKey = keyof typeof uk;

/** The Ukrainian text for `key`, with every `{name}` replaced from `vars`. */
export function t(key: MessageKey, vars: Record<string, string> = {}): string {
  return Object.entries(vars).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, value),
    uk[key],
  );
}
