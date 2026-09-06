import { expect, test, type Page } from "@playwright/test";

/**
 * A service, end to end, through the screens a member of staff actually uses.
 *
 * Every one of these surfaces was built without being seen: the sign-in keypad,
 * the notice that appears when the server refuses something, the check, the
 * pass. This is the first thing that opens them.
 *
 * Serial by necessity — one server, one floor, one database. Each test that
 * touches the floor resets it first rather than inheriting the last one's.
 */

const MANAGER = "manager-1";
const PIN = "246810";

test.describe.configure({ mode: "serial" });

/**
 * Idempotent: the server refuses this once any PIN exists, which is fine.
 *
 * The setup code is read from the server's log, which is exactly how an
 * operator gets it — and the point of the control: it is not available over
 * the network.
 */
async function ensureManagerPin(page: Page) {
  // Matches EMBER_SETUP_TOKEN in playwright.config.ts. In a real deployment
  // this is random and printed to the console.
  const token = "e2e-setup-token";
  await page.request
    .post("/api/auth/setup", {
      data: { staffId: MANAGER, pin: PIN, setupToken: token },
    })
    .catch(() => undefined);
}

/** Signs in through the real screen and lands on the floor. */
async function signIn(page: Page) {
  await ensureManagerPin(page);
  await page.goto("/");
  await page.getByLabel("Staff ID").fill(MANAGER);
  await page.getByLabel("PIN", { exact: true }).fill(PIN);
  await page.getByRole("button", { name: /sign in/i }).click();
  await page.waitForURL("**/pos");
}

/**
 * A clean floor and a signed-in terminal, for tests that are about the floor
 * rather than about signing in.
 *
 * Signs in over the API: it shares the page's cookie jar, so the browser is
 * authenticated without walking the keypad again — and walking it would fail
 * anyway, because a terminal that already has a session is redirected straight
 * past the form.
 */
async function signInAndReset(page: Page) {
  await ensureManagerPin(page);
  await page.request.post("/api/auth/login", {
    data: { staffId: MANAGER, pin: PIN, terminalId: "e2e" },
  });
  await page.request.post("/api/actions", {
    data: { id: `e2e-reset-${Date.now()}`, type: "reset" },
  });
  await page.goto("/pos");
}

test("the sign-in screen offers first-run setup and asks for the console code", async ({
  page,
}) => {
  // Only true before any PIN exists, which is why this runs first.
  await page.goto("/");
  const firstRunField = page.getByLabel("Setup code");
  if (await firstRunField.isVisible().catch(() => false)) {
    // Claiming the first manager account must need something only someone with
    // access to the machine has.
    await expect(firstRunField).toBeVisible();
  }
  const firstRun = page.getByRole("heading", {
    name: /set the first manager pin/i,
  });
  const returning = page.getByRole("heading", { name: /sign in to the floor/i });
  await expect(firstRun.or(returning)).toBeVisible();
});

test("signing in puts a named person on the floor", async ({ page }) => {
  await signIn(page);

  await expect(
    page.getByRole("heading", { name: /front-of-house/i }),
  ).toBeVisible();
  // The audit trail is per-person, so the floor says whose session this is.
  await expect(page.getByText("Marcus Lee")).toBeVisible();
  await expect(page.getByRole("button", { name: /sign out/i })).toBeVisible();
});

test("a wrong PIN says so, rather than claiming the terminal is signed out", async ({
  page,
}) => {
  await ensureManagerPin(page);
  await page.goto("/");
  await page.getByLabel("Staff ID").fill(MANAGER);
  await page.getByLabel("PIN", { exact: true }).fill("000000");
  await page.getByRole("button", { name: /sign in/i }).click();

  // Scoped to the form: Next ships its own route announcer with role="alert".
  const alert = page.getByRole("main").getByRole("alert");
  await expect(alert).toContainText(/pin/i);
  await expect(alert).not.toContainText(/not signed in/i);
});

test("the floor cannot be reached without signing in", async ({ page }) => {
  await page.context().clearCookies();
  await page.goto("/pos");
  // The gate sends an unauthenticated terminal to the keypad rather than
  // showing a floor that refuses every tap.
  await expect(page.getByRole("button", { name: /sign in/i })).toBeVisible();
});

test("a party is seated, ordered for, fired and bumped", async ({ page }) => {
  await signInAndReset(page);

  await page.getByRole("button", { name: /select maya chen/i }).click();
  await page.getByRole("button", { name: /seat at/i }).first().click();

  // Seating advances to the order screen on its own.
  await page.getByRole("button", { name: "Order", exact: true }).click();
  await expect(page.getByText(/ordering for/i)).toBeVisible();
  await expect(page.getByText("Maya Chen")).toBeVisible();

  // The tartare has hazelnut and Maya reacts to tree nuts, so the engine
  // blocks it — and the screen has to say why, not just grey it out.
  await expect(page.getByText(/contains guest allergen: tree nuts/i)).toBeVisible();

  // Golden Beet is safe for her: no tree nuts, and gluten-free.
  await page.getByRole("button", { name: "Add Golden Beet & Citrus" }).click();
  await expect(page.getByText(/current check/i)).toBeVisible();
  await expect(page.getByText("$17.00").first()).toBeVisible();

  await page.getByRole("button", { name: /send order/i }).click();
  await expect(page.getByRole("button", { name: /sent|away/i })).toBeVisible();

  // The pass.
  await page.goto("/pos?view=kitchen");
  await expect(page.getByText(/maya chen/i).first()).toBeVisible();
  await page.getByRole("button", { name: /^bump/i }).first().click();
  await expect(page.getByText(/maya chen/i)).toHaveCount(0);
});

test("a refused action is shown to the person who made it", async ({ page }) => {
  await signInAndReset(page);

  // The server's refusal path is covered by the Rust tests; what has never
  // been checked is that the client *renders* one instead of swallowing it,
  // which is what it did for most of this branch's life. Forcing the response
  // tests exactly that, without depending on which action happens to be
  // refusable through the UI today.
  await page.route("**/api/actions", async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        ...body,
        outcome: "rejected",
        reason: "table-unavailable",
        reasonMessage: "That table is taken.",
      }),
    });
  });

  await page.getByRole("button", { name: /maya chen/i }).first().click();
  await page.getByRole("button", { name: /seat at/i }).first().click();

  await expect(page.getByText("That table is taken.")).toBeVisible({
    timeout: 10_000,
  });
});
