import { describe, expect, it } from "vitest";
import { t } from "./i18n";

describe("t", () => {
  it("reads the shared table", () => {
    expect(t("app_title")).toBe("AutoPara — автозапуск пар");
  });

  it("fills placeholders", () => {
    expect(t("settings_version", { version: "2.0.0" })).toBe("Версія 2.0.0");
  });
});
