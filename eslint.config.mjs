import { defineConfig, globalIgnores } from "eslint/config";
import nextVitals from "eslint-config-next/core-web-vitals";
import nextTs from "eslint-config-next/typescript";

const eslintConfig = defineConfig([
  ...nextVitals,
  ...nextTs,
  // Override default ignores of eslint-config-next.
  globalIgnores([
    // Default ignores of eslint-config-next:
    ".next/**",
    "out/**",
    "build/**",
    "next-env.d.ts",
    // Cargo's build directory. Tauri's codegen writes bundled assets as .js
    // there, which ESLint would otherwise try to parse.
    "target/**",
    // Generated from the Rust types by ts-rs; edit crates/ember-core instead.
    "lib/generated/**",
  ]),
]);

export default eslintConfig;
