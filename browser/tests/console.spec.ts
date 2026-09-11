// Browser-observable proofs for the Card console (PER-84, S3). These are
// the proofs a parsed DOM cannot give: what the browser fetched, what the
// operator can see, where navigation actually lands, whether the page
// errors. Server-output equality against `card history` lives in the Rust
// behavior suite; this file asserts on visibility and behavior, and reads
// the same durable record for expected values.

import { expect, test, type Page, type Request } from "@playwright/test";
import { byField, byRole, cardRoute, Console, entryRoute, Fixture, field, role } from "./console";

interface World {
  fixture: Fixture;
  console: Console;
  /** The primary Card: card-created, a decision, and a stage event. */
  card: string;
  decision: { entry_id: string; sequence: number };
  stageEvent: { entry_id: string; sequence: number };
  /** A second Card with no task key, and a third; the queue must show all. */
  others: string[];
}

async function openWorld(): Promise<World> {
  const fixture = new Fixture().migrate();
  try {
    const card = fixture.createCard("Dogfood card", ["--task-key", "PER-84", "--workspace", "github/daemar"]);
    const decision = fixture.appendDecision(card, "Chose loopback", "S3-B2 pins it");
    const stageEvent = fixture.appendStageEvent(card, "codex", "agent", "review", "Round one clean");
    const others = [
      fixture.createCard("Second card"),
      fixture.createCard("Third card", ["--task-key", "PER-103"]),
    ];
    const console = await Console.start(fixture);
    return { fixture, console, card, decision, stageEvent, others };
  } catch (error) {
    // Nothing owns the fixture yet, so a failed start disposes it here.
    fixture.dispose();
    throw error;
  }
}

async function closeWorld(world: World) {
  await world.console.stop();
  world.fixture.dispose();
}

/** Asserts a rendered value is visible to the operator and reads as `text`. */
async function visibleText(scope: import("@playwright/test").Locator, name: string, text: string) {
  const shown = scope.locator(byField(name));
  await expect(shown, `data-field=${name}`).toBeVisible();
  await expect(shown, `data-field=${name}`).toHaveText(text);
}

/**
 * Asserts a rendered timestamp is visible, carries the durable instant, and
 * reads as some text: a styled empty <time> is visible to a browser and says
 * nothing to the operator. The human spelling is not pinned by the spec.
 */
async function visibleInstant(scope: import("@playwright/test").Locator, name: string, instant: string) {
  const shown = scope.locator(byField(name));
  await expect(shown, `data-field=${name}`).toBeVisible();
  await expect(shown, `data-field=${name}`).toHaveAttribute("datetime", instant);
  await expect(shown, `data-field=${name} reads as text`).toHaveText(/\S/);
}

/** Every request the page issued while loading, and every error it raised. */
function observe(page: Page) {
  const requests: Request[] = [];
  const errors: string[] = [];
  page.on("request", (request) => requests.push(request));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  page.on("pageerror", (error) => errors.push(error.message));
  return { requests, errors };
}

test.describe("S3-B8 the console is self-contained in a real browser", () => {
  for (const [name, route] of [
    ["the queue page", (w: World) => "/"],
    ["a Card page", (w: World) => cardRoute(w.card)],
    ["an entry inspector", (w: World) => entryRoute(w.card, w.decision.entry_id)],
  ] as const) {
    test(`loading ${name} fetches exactly the page and one stylesheet, with no errors`, async ({ page }) => {
      const world = await openWorld();
      try {
        const seen = observe(page);
        const response = await page.goto(world.console.url + route(world));
        expect(response?.status()).toBe(200);
        await page.waitForLoadState("load");

        const origin = new URL(world.console.url).origin;
        const fetched = seen.requests.map((request) => new URL(request.url()));
        for (const url of fetched) {
          expect(url.origin, `${url.href} left the console origin`).toBe(origin);
        }
        const kinds = seen.requests.map((request) => request.resourceType()).sort();
        expect(kinds).toEqual(["document", "stylesheet"]);
        expect(seen.errors).toEqual([]);
      } finally {
        await closeWorld(world);
      }
    });
  }
});

/** S3-B5: the five Card identity values are each visibly rendered. */
async function expectIdentityVisible(page: Page, world: World) {
  const identity = page.locator(byRole(role.cardIdentity));
  await expect(identity).toBeVisible();
  const [summary] = world.fixture.list().filter((card) => card.card_id === world.card);
  await visibleText(identity, field.cardId, summary.card_id);
  await visibleText(identity, field.title, summary.title);
  await visibleText(identity, field.taskKey, summary.task_key ?? "");
  await visibleText(identity, field.workspace, summary.workspace ?? "");
  await visibleInstant(identity, field.createdAt, summary.created_at);
}

test.describe("S3-B4/B5 the operator can read the queue and the stream", () => {
  test("every queue row's title, task key, and last activity are visible and match the record", async ({ page }) => {
    const world = await openWorld();
    try {
      await page.goto(world.console.url + "/");
      const cards = world.fixture.list();
      expect(cards.length).toBe(3);
      const rows = page.locator(byRole(role.queueRow));
      await expect(rows).toHaveCount(cards.length);
      for (const card of cards) {
        const row = page.locator(`${byRole(role.queueRow)}[data-card-id="${card.card_id}"]`);
        await expect(row, card.title).toBeVisible();
        await visibleText(row, field.title, card.title);
        if (card.task_key === null) {
          // An absent task key is omitted or blank, never a stand-in.
          const key = row.locator(byField(field.taskKey));
          if ((await key.count()) > 0) {
            await expect(key, "absent task key").toHaveText("");
          }
        } else {
          await visibleText(row, field.taskKey, card.task_key);
        }
        const latest = world.fixture.history(card.card_id).at(-1)!;
        await visibleInstant(row, field.lastActivity, latest.recorded_at);
      }
    } finally {
      await closeWorld(world);
    }
  });

  test("every stream row is visible with its sequence, summary, and provenance", async ({ page }) => {
    const world = await openWorld();
    try {
      await page.goto(world.console.url + cardRoute(world.card));
      await expectIdentityVisible(page, world);
      const history = world.fixture.history(world.card);
      const rows = page.locator(byRole(role.streamRow));
      await expect(rows).toHaveCount(history.length);
      await expect(rows.first()).toHaveCSS("border-bottom-width", "1px");
      // S3-B5: every row shows sequence, entry type, producer identity,
      // producer kind, recorded_at, and the reported label.
      for (const entry of history) {
        const row = page.locator(`${byRole(role.streamRow)}[data-entry-id="${entry.entry_id}"]`);
        await expect(row).toBeVisible();
        await visibleText(row, field.sequence, String(entry.sequence));
        await visibleText(row, field.entryType, entry.entry_type);
        await visibleText(row, field.producerId, entry.producer.id);
        await visibleText(row, field.producerKind, entry.producer.kind);
        await visibleInstant(row, field.recordedAt, entry.recorded_at);
        await visibleText(row, field.provenance, "reported");
      }
      // Each entry type summarizes visibly in its own shape.
      const rowFor = (entryId: string) => page.locator(`${byRole(role.streamRow)}[data-entry-id="${entryId}"]`);
      await visibleText(rowFor(history[0].entry_id), field.summary, "Dogfood card");
      await visibleText(rowFor(world.decision.entry_id), field.summary, "Chose loopback");
      await visibleText(rowFor(world.stageEvent.entry_id), field.stage, "review");
      await visibleText(rowFor(world.stageEvent.entry_id), field.summary, "Round one clean");
    } finally {
      await closeWorld(world);
    }
  });
});

test.describe("S3-B6 the inspector is reachable by clicking and its envelope is visible", () => {
  test("clicking a stream row's inspect link lands on the entry route with a visible envelope", async ({ page }) => {
    const world = await openWorld();
    try {
      await page.goto(world.console.url + cardRoute(world.card));
      const row = page.locator(`${byRole(role.streamRow)}[data-entry-id="${world.decision.entry_id}"]`);
      await row.locator(byRole(role.inspect)).click();
      await expect(page).toHaveURL(world.console.url + entryRoute(world.card, world.decision.entry_id));

      const [entry] = world.fixture.history(world.card).filter((e) => e.entry_id === world.decision.entry_id);
      const inspector = page.locator(byRole(role.inspector));
      await expect(inspector).toBeVisible();
      const envelope: Array<[string, string]> = [
        [field.entryId, entry.entry_id],
        [field.cardId, entry.card_id],
        [field.sequence, String(entry.sequence)],
        [field.schemaVersion, String(entry.schema_version)],
        [field.entryType, entry.entry_type],
        [field.producerId, entry.producer.id],
        [field.producerKind, entry.producer.kind],
      ];
      for (const [name, value] of envelope) {
        const shown = inspector.locator(byField(name));
        await expect(shown, `data-field=${name}`).toBeVisible();
        await expect(shown, `data-field=${name}`).toHaveText(value);
      }
      await visibleInstant(inspector, field.recordedAt, entry.recorded_at);
      await expect(inspector.locator(byRole(role.payload))).toBeVisible();
      await expect(inspector.locator(byRole(role.payload))).toContainText("Chose loopback");

      // The queue and the Card identity stay on screen around the inspector.
      await expect(page.locator(byRole(role.queue))).toBeVisible();
      await expectIdentityVisible(page, world);
      const selected = page.locator(`${byRole(role.queueRow)}[aria-current="page"]`);
      await expect(selected).toBeVisible();
      await expect(selected).toContainText("Dogfood card");
    } finally {
      await closeWorld(world);
    }
  });

  test("browser back from the inspector returns to the Card page with no inspector", async ({ page }) => {
    const world = await openWorld();
    try {
      await page.goto(world.console.url + cardRoute(world.card));
      await page
        .locator(`${byRole(role.streamRow)}[data-entry-id="${world.decision.entry_id}"] ${byRole(role.inspect)}`)
        .click();
      await expect(page.locator(byRole(role.inspector))).toBeVisible();
      await page.goBack();
      await expect(page).toHaveURL(world.console.url + cardRoute(world.card));
      await expect(page.locator(byRole(role.inspector))).toHaveCount(0);
      await expect(page.locator(byRole(role.stream))).toBeVisible();
    } finally {
      await closeWorld(world);
    }
  });
});

test.describe("S3-B9 failures are visible and fabricate nothing", () => {
  test("an unknown Card is a visible not-found page with the queue and no Card identity", async ({ page }) => {
    const world = await openWorld();
    try {
      const response = await page.goto(world.console.url + cardRoute("01a00000-0000-7000-8000-000000000000"));
      expect(response?.status()).toBe(404);
      const error = page.locator(byRole(role.error));
      await expect(error).toBeVisible();
      await expect(error).toContainText(/not found/i);
      await expect(page.locator(byRole(role.queue))).toBeVisible();
      await expect(page.locator(byRole(role.cardIdentity))).toHaveCount(0);
      await expect(page.locator(byRole(role.stream))).toHaveCount(0);
    } finally {
      await closeWorld(world);
    }
  });

  test("a corrupt entry is a visible storage-failed page that keeps the queue and fabricates nothing", async ({ page }) => {
    const world = await openWorld();
    try {
      world.fixture.corruptPayload(world.card, world.decision.sequence);
      const response = await page.goto(world.console.url + cardRoute(world.card));
      expect(response?.status()).toBe(500);
      const error = page.locator(byRole(role.error));
      await expect(error).toBeVisible();
      await expect(error).toContainText(/storage failed/i);
      await expect(page.locator(byRole(role.streamRow))).toHaveCount(0);
      await expect(page.locator(byRole(role.inspector))).toHaveCount(0);
      await expect(page.locator(byRole(role.payload))).toHaveCount(0);
      for (const fabricated of ["{}", "null", "{not json"]) {
        await expect(page.locator("body")).not.toContainText(fabricated);
      }
      const queue = page.locator(byRole(role.queue));
      await expect(queue).toBeVisible();
      await expect(queue.locator(`${byRole(role.queueRow)}[data-card-id="${world.card}"]`)).toBeVisible();
    } finally {
      await closeWorld(world);
    }
  });

  test("a queue the console cannot read is a visible storage-failed page with no queue", async ({ page }) => {
    const world = await openWorld();
    try {
      const [beta] = world.others;
      world.fixture.corruptCreatedRecordedAt(beta);
      const response = await page.goto(world.console.url + "/");
      expect(response?.status()).toBe(500);
      const error = page.locator(byRole(role.error));
      await expect(error).toBeVisible();
      await expect(error).toContainText(/storage failed/i);
      await expect(page.locator(byRole(role.queue))).toHaveCount(0);
      await expect(page.locator(byRole(role.queueRow))).toHaveCount(0);
      await expect(page.locator(byRole(role.cardIdentity))).toHaveCount(0);
      await expect(page.locator(byRole(role.stream))).toHaveCount(0);
      await expect(page.locator("body")).not.toContainText("Dogfood card");
    } finally {
      await closeWorld(world);
    }
  });

  test("an empty, migrated store shows a visibly empty queue", async ({ page }) => {
    // S3-B4: a migrated store with no Cards, not a missing file (S3-B3).
    const fixture = new Fixture().migrate();
    let console: Console | undefined;
    try {
      console = await Console.start(fixture);
      await page.goto(console.url + "/");
      await expect(page.locator(byRole(role.queueEmpty))).toBeVisible();
      await expect(page.locator(byRole(role.queueRow))).toHaveCount(0);
    } finally {
      await console?.stop();
      fixture.dispose();
    }
  });
});
