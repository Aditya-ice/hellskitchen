import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: {
    alias: {
      "@": fileURLToPath(new URL(".", import.meta.url)),
    },
  },
  test: {
    environment: "node",
    // Playwright owns e2e/. Vitest picking those files up fails at import,
    // because a spec calling test.describe.configure() outside the Playwright
    // runner is an error rather than a skip.
    exclude: ["node_modules/**", "e2e/**", "out/**", ".next/**", "target/**"],
  },
});
