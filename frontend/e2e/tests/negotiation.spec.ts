// The full negotiation, end to end, in two authenticated browser contexts:
// studio offers (with a dragged playback window) → label counters from the
// movie-context page → studio accepts → both sides see ACCEPTED, live via
// SSE, with Mailpit carrying the emails. Also pins the login redirect and
// upload-at-creation flows.
//
// Requires the compose stack and seed data: `just up && just seed`.

import { expect, test, type Page } from "@playwright/test";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const STUDIO = { email: "grace@acme.example", password: "nw-derulo-99" };
const LABEL = { email: "warp-label@acme.example", password: "label-pass-99" };

const OFFER_FEE = "123.45";
const COUNTER_FEE = "222.00";

async function login(page: Page, creds: { email: string; password: string }) {
  await page.goto("/login");
  await page.getByLabel("Email").fill(creds.email);
  await page.getByLabel("Password").fill(creds.password);
  await page.locator('form button[type="submit"]').click();
}

/** Current unread badge count (0 when the badge is hidden). */
async function badgeCount(page: Page): Promise<number> {
  const badge = page.locator(".bell-badge");
  if ((await badge.count()) === 0) return 0;
  const text = await badge.first().textContent();
  return Number.parseInt((text ?? "0").replace("+", ""), 10) || 0;
}

/** Zero the badge so the next event is unambiguous. */
async function markAllRead(page: Page) {
  await page.locator(".bell-button").click();
  const markAll = page.getByRole("button", { name: "Mark all read" });
  if ((await markAll.count()) > 0) {
    await markAll.click();
    await expect(page.locator(".bell-badge")).toHaveCount(0);
  }
  await page.locator(".bell-button").click(); // close
}

/** Mailpit message bodies, newest first. */
async function mailpitBodies(): Promise<string[]> {
  const response = await fetch("http://localhost:8025/api/v1/messages?limit=50");
  const list = (await response.json()) as {
    messages: { ID: string }[];
  };
  const bodies = await Promise.all(
    list.messages.map(async ({ ID }) => {
      const detail = await fetch(`http://localhost:8025/api/v1/message/${ID}`);
      const message = (await detail.json()) as { Subject: string; Text: string };
      return `${message.Subject}\n${message.Text}`;
    }),
  );
  return bodies;
}

test("full negotiation with live notifications and email", async ({ browser }) => {
  const title = `E2E Neon Pursuit ${Date.now()}`;

  // ── Label context: settle the inbox so later events are unambiguous ──
  const labelContext = await browser.newContext();
  const label = await labelContext.newPage();
  await login(label, LABEL);
  await expect(label).toHaveURL(/\/catalog$/);
  await markAllRead(label);

  // ── Studio context: login lands on the movies home (redirect regression) ──
  const studioContext = await browser.newContext();
  const studio = await studioContext.newPage();
  await login(studio, STUDIO);
  await expect(studio).toHaveURL(/\/movies$/);

  // Create a movie with a poster attached at creation time.
  const poster = path.join(os.tmpdir(), "e2e-poster.png");
  fs.writeFileSync(poster, Buffer.from(PNG, "base64"));
  await studio.getByPlaceholder("Neon Pursuit").fill(title);
  await studio
    .getByPlaceholder("A rain-soaked chase through a neon city.")
    .fill("The e2e chase.");
  await studio.locator('input[type="file"]').first().setInputFiles(poster);
  await studio.getByRole("button", { name: "Add movie" }).click();
  const card = studio.locator(".grid a.card", { hasText: title }).first();
  await expect(card).toBeVisible();
  await expect(card.locator(`img[alt="${title} poster"]`)).toBeVisible();

  // Open it and give it a scene to license into.
  await card.click();
  await expect(studio.getByRole("heading", { name: title })).toBeVisible();
  const movieUrl = studio.url();
  await studio.getByLabel("Screen time (s)").fill("120");
  await studio.getByLabel("Description").fill("The decision scene");
  await studio.getByRole("button", { name: "Add scene" }).click();
  await expect(studio.getByText("The decision scene").first()).toBeVisible();

  // Find music: suggestions show before typing, then search.
  await studio.getByRole("button", { name: "Find music" }).click();
  await expect(studio.getByText("Fresh in the catalog")).toBeVisible();
  await studio
    .getByPlaceholder(/Search the catalog/)
    .fill("turbo");
  await studio.getByRole("heading", { name: "Turbo Killer" }).first().waitFor();
  // Scope to the card: fuzzy search also surfaces "Toro y Pampa" for
  // "turbo", and an unscoped .first() clicks whichever ranks higher.
  await studio
    .locator(".card", { hasText: "Turbo Killer" })
    .getByRole("button", { name: "License this song" })
    .click();

  // Offer modal: drag the window's end handle, then send a distinctive fee.
  await expect(studio.getByText(/License “Turbo Killer”/)).toBeVisible();
  const handle = studio.getByLabel("window end");
  const box = await handle.boundingBox();
  expect(box).not.toBeNull();
  await studio.mouse.move(box!.x + box!.width / 2, box!.y + box!.height / 2);
  await studio.mouse.down();
  await studio.mouse.move(box!.x + 120, box!.y, { steps: 6 });
  await studio.mouse.up();
  await expect(studio.getByLabel("End (s)")).not.toHaveValue("30");
  await studio.getByLabel("Offer fee (USD)").fill(OFFER_FEE);
  await studio.getByRole("button", { name: "Send offer" }).click();
  await expect(studio.getByText(/License “Turbo Killer”/)).toHaveCount(0);

  // The board shows the offer after a fresh visit (also proves session
  // restore — the movie page renders without re-authenticating).
  await studio.goto(movieUrl);
  await expect(
    studio.locator("tr", { hasText: "Turbo Killer" }).filter({ hasText: "Offer out" }),
  ).toBeVisible();

  // ── Label: the badge moves live over SSE, without a reload ──
  await expect
    .poll(() => badgeCount(label), { timeout: 30_000 })
    .toBeGreaterThanOrEqual(1);
  await label.locator(".bell-button").click();
  const row = label
    .locator(".notif", { hasText: "offer received" })
    .filter({ hasText: "Turbo Killer" })
    .filter({ hasText: title })
    .first();
  await expect(row).toBeVisible();

  // Click-through lands on the decision page with the whole movie context.
  await row.click();
  await expect(label).toHaveURL(/\/movies\/.+\/context$/);
  const counterRow = label
    .locator("tr", { hasText: "Turbo Killer" })
    .filter({ hasText: "Offer out" })
    .first();
  await expect(counterRow).toBeVisible();

  // Counter from here.
  await counterRow.getByRole("button", { name: "Counter" }).click();
  await counterRow.locator(".fee-input input").fill(COUNTER_FEE);
  await counterRow.getByRole("button", { name: "Send" }).click();
  await expect(
    label.locator("tr", { hasText: "Turbo Killer" }).filter({ hasText: "Countered" }),
  ).toBeVisible();

  // ── Studio: counter lands live; accept it ──
  await expect
    .poll(() => badgeCount(studio), { timeout: 30_000 })
    .toBeGreaterThanOrEqual(1);
  await studio.locator(".bell-button").click();
  const counterNotice = studio
    .locator(".notif", { hasText: "counter-offer received" })
    .filter({ hasText: "Turbo Killer" })
    .first();
  await expect(counterNotice).toBeVisible();
  await counterNotice.click();
  await expect(studio).toHaveURL(/\/movies\/[^/]+$/);
  const acceptedRow = studio
    .locator("tr", { hasText: "Turbo Killer" })
    .filter({ hasText: "Countered" })
    .first();
  await acceptedRow.getByRole("button", { name: "Accept counter" }).click();
  await expect(
    studio.locator("tr", { hasText: "Turbo Killer" }).filter({ hasText: "Accepted" }),
  ).toBeVisible();

  // ── Label: acceptance arrives live on the open context page ──
  await expect(
    label.locator("tr", { hasText: "Turbo Killer" }).filter({ hasText: "Accepted" }),
  ).toBeVisible({ timeout: 30_000 });

  // ── Mailpit carried the negotiation by email ──
  const bodies = await mailpitBodies();
  expect(
    bodies.filter((body) => body.includes(`Current fee: $${OFFER_FEE}.`)).length,
  ).toBeGreaterThanOrEqual(1);
  expect(
    bodies.filter((body) => body.includes(`Current fee: $${COUNTER_FEE}.`)).length,
  ).toBeGreaterThanOrEqual(1);
});

// 1×1 transparent PNG.
const PNG =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
