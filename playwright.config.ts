import { defineConfig, devices } from "@playwright/test";

/**
 * End-to-end tests against the real binary.
 *
 * These exist because every UI change on this branch was, until now, verified
 * only by typecheck, lint and the production build — none of which can tell you
 * that a screen renders, that a form submits, or that a button does anything.
 *
 * `npm start` is what a venue runs: it builds the UI and serves it and the API
 * from one Rust binary on one port. Testing that rather than `next dev` means
 * the thing under test is the thing that ships.
 */
export default defineConfig({
  testDir: "./e2e",
  fullyParallel: false,
  // The service is shared state on one port; parallel workers would fight over
  // the same floor.
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? "github" : "list",
  timeout: 30_000,

  use: {
    baseURL: "http://127.0.0.1:4000",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },

  projects: [
    {
      name: "chrome",
      // Locally this drives the Chrome already installed, so a checkout does
      // not pay for a browser download. CI has no Chrome, so it installs
      // Playwright's Chromium and asks for it by name.
      use: {
        ...devices["Desktop Chrome"],
        ...(process.env.PLAYWRIGHT_BROWSER === "chromium"
          ? {}
          : { channel: "chrome" }),
      },
    },
  ],

  webServer: {
    // A fresh database per run, so a test never inherits a previous floor.
    //
    // EMBER_SETUP_TOKEN is a test-only convenience: the real bootstrap code is
    // random and printed to the console, so a spec has no way to learn it.
    // Seeding the manager PIN up front skips that path, which has its own
    // coverage in the Rust tests.
    command:
      "rm -f .e2e/ember.db* && EMBER_DB=.e2e/ember.db EMBER_SETUP_TOKEN=e2e-setup-token EMBER_STATIC_DIR=./out ./target/release/ember-server",
    url: "http://127.0.0.1:4000/api/health",
    reuseExistingServer: false,
    timeout: 120_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});
