import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist/", "src-tauri/", "ui/src/bindings/"] },
  js.configs.recommended,
  ...tseslint.configs.strict,
);
