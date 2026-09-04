import { defineConfig } from "@playwright/test";

// Runs against the live compose stack (frontend :5173, gateway :8080,
// Mailpit :8025). One worker: the specs drive shared stack state.
export default defineConfig({
  testDir: "tests",
  timeout: 60_000,
  expect: { timeout: 15_000 },
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [["list"]],
  use: {
    baseURL: "http://localhost:5173",
    trace: "retain-on-failure",
  },
  projects: [{ name: "chromium", use: { browserName: "chromium" } }],
});
