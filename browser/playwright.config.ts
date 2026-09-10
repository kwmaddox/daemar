import { defineConfig, devices } from "@playwright/test";

// Browser-observable proofs for the Card console (PER-84, S3). Each test
// spawns its own `card serve` on a scenario-local database, so tests are
// independent and may run in parallel. Runs via `just browser`; the
// Rust behavior suite (cucumber) remains the pre-commit gate.
export default defineConfig({
  testDir: "./tests",
  fullyParallel: true,
  forbidOnly: true,
  retries: 0,
  reporter: [["list"], ["html", { open: "never" }]],
  use: {
    ...devices["Desktop Chrome"],
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
});
