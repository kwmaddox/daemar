// The console under test as a process, plus the `card` CLI as the fixture
// and oracle. Expected values never come from the page: they come from
// `card history`, the durable record, and the page must agree with it.

import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { DatabaseSync } from "node:sqlite";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

/** The compiled binary; `just browser` builds it first. */
export const CARD_BIN = resolve(__dirname, "../../target/debug/card");

const STARTUP_TIMEOUT_MS = 15_000;

export interface Entry {
  card_id: string;
  entry_id: string;
  entry_type: string;
  payload: Record<string, unknown>;
  producer: { id: string; kind: string };
  recorded_at: string;
  schema_version: number;
  sequence: number;
}

/** What `card append` prints: the receipt, not the entry. */
export interface Appended {
  card_id: string;
  entry_id: string;
  sequence: number;
}

export interface CardSummary {
  card_id: string;
  title: string;
  task_key: string | null;
  workspace: string | null;
  created_at: string;
}

/** The markup contract the Rust suite pins: landmarks and rendered values. */
export const role = {
  queue: "queue",
  queueRow: "queue-row",
  queueEmpty: "queue-empty",
  cardIdentity: "card-identity",
  stream: "stream",
  streamRow: "stream-row",
  inspect: "inspect",
  inspector: "inspector",
  payload: "payload",
  error: "error",
} as const;

export const field = {
  cardId: "card-id",
  title: "title",
  taskKey: "task-key",
  workspace: "workspace",
  createdAt: "created-at",
  lastActivity: "last-activity",
  sequence: "sequence",
  entryId: "entry-id",
  entryType: "entry-type",
  schemaVersion: "schema-version",
  producerId: "producer-id",
  producerKind: "producer-kind",
  recordedAt: "recorded-at",
  summary: "summary",
  reason: "reason",
  stage: "stage",
  provenance: "provenance",
} as const;

export const byRole = (name: string) => `[data-role="${name}"]`;
export const byField = (name: string) => `[data-field="${name}"]`;

/** One scenario-local database and the CLI bound to it. */
export class Fixture {
  readonly dir: string;
  readonly db: string;

  constructor() {
    this.dir = mkdtempSync(join(tmpdir(), "daemar-browser-"));
    this.db = join(this.dir, "cards.db");
  }

  /**
   * Creates and migrates the database through the real CLI, so an
   * empty-queue scenario starts from a migrated empty store (S3-B4) and
   * not from a missing file (S3-B3 rejects that).
   */
  migrate(): this {
    const cards = this.list();
    if (cards.length !== 0) throw new Error("fresh scenario database must be empty");
    return this;
  }

  private run(args: string[]): string {
    const result = spawnSync(CARD_BIN, ["--db", this.db, ...args], { encoding: "utf8" });
    if (result.status !== 0) {
      throw new Error(`card ${args.join(" ")} failed (${result.status}):\n${result.stderr}`);
    }
    return result.stdout;
  }

  createCard(title: string, extra: string[] = []): string {
    const out = this.run([
      "create",
      "--producer",
      "test-operator",
      "--producer-kind",
      "operator",
      "--title",
      title,
      ...extra,
    ]);
    return (JSON.parse(out) as { card_id: string }).card_id;
  }

  appendDecision(card: string, summary: string, reason: string): Appended {
    const out = this.run([
      "append",
      card,
      "--entry-type",
      "decision",
      "--producer",
      "claude",
      "--producer-kind",
      "agent",
      "--payload",
      JSON.stringify({ summary, reason }),
    ]);
    return JSON.parse(out) as Appended;
  }

  appendStageEvent(card: string, producer: string, kind: string, stage: string, summary: string): Appended {
    const out = this.run([
      "append",
      card,
      "--entry-type",
      "stage-event",
      "--stage",
      stage,
      "--summary",
      summary,
      "--producer",
      producer,
      "--producer-kind",
      kind,
    ]);
    return JSON.parse(out) as Appended;
  }

  /**
   * S3-B9 fixtures: invalidate one durable value through a separate
   * writable connection, the same shape the Rust suite uses (review
   * finding G1: never swap the file under the console). Exactly one row
   * must change or the fixture, not the console, is wrong.
   */
  private corrupt(sql: string, params: Array<string | number>) {
    const db = new DatabaseSync(this.db);
    try {
      const changed = db.prepare(sql).run(...params).changes;
      if (changed !== 1) throw new Error(`corrupted ${changed} rows, expected 1: ${sql}`);
    } finally {
      db.close();
    }
  }

  corruptPayload(card: string, sequence: number) {
    this.corrupt("UPDATE card_entries SET payload = ? WHERE card_id = ? AND sequence = ?", [
      "{not json",
      card,
      sequence,
    ]);
  }

  corruptCreatedRecordedAt(card: string) {
    this.corrupt(
      "UPDATE card_entries SET recorded_at = ? WHERE card_id = ? AND sequence = 1 AND entry_type = ?",
      ["not-a-timestamp", card, "card-created"],
    );
  }

  history(card: string): Entry[] {
    return (JSON.parse(this.run(["history", card])) as { entries: Entry[] }).entries;
  }

  list(): CardSummary[] {
    return (JSON.parse(this.run(["list"])) as { cards: CardSummary[] }).cards;
  }

  dispose() {
    rmSync(this.dir, { recursive: true, force: true });
  }
}

/** A running `card serve`, started on an ephemeral loopback port. */
export class Console {
  private constructor(
    private readonly child: ChildProcess,
    readonly url: string,
    readonly startup: Record<string, string>,
  ) {}

  static async start(fixture: Fixture): Promise<Console> {
    const child = spawn(CARD_BIN, ["serve", "--port", "0"], {
      env: { ...process.env, DAEMAR_DB: fixture.db },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stderr = "";
    child.stderr?.on("data", (chunk: Buffer) => (stderr += chunk.toString()));
    try {
      const startupLine = await new Promise<string>((resolveLine, reject) => {
        let buffered = "";
        const timer = setTimeout(
          () => reject(new Error(`no startup line within ${STARTUP_TIMEOUT_MS}ms\n${stderr}`)),
          STARTUP_TIMEOUT_MS,
        );
        child.stdout?.on("data", (chunk: Buffer) => {
          buffered += chunk.toString();
          const newline = buffered.indexOf("\n");
          if (newline >= 0) {
            clearTimeout(timer);
            resolveLine(buffered.slice(0, newline));
          }
        });
        child.on("exit", (code) => {
          clearTimeout(timer);
          reject(new Error(`the console exited (${code}) instead of serving\n${stderr}`));
        });
      });
      const startup = JSON.parse(startupLine) as Record<string, string>;
      return new Console(child, startup.url.replace(/\/$/, ""), startup);
    } catch (error) {
      // Setup is transactional: a rejected start never leaves a child
      // behind, whether it exited on its own or timed out still running.
      await terminate(child);
      throw error;
    }
  }

  /** Kills the server and waits until it has actually gone. */
  stop(): Promise<void> {
    return terminate(this.child);
  }
}

async function terminate(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return;
  await new Promise<void>((resolveExit) => {
    child.once("exit", () => resolveExit());
    child.kill();
  });
}

export const cardRoute = (card: string) => `/cards/${card}`;
export const entryRoute = (card: string, entry: string) => `/cards/${card}?entry=${entry}`;
