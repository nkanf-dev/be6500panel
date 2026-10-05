import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: false,
  workers: 1,
  timeout: 30_000,
  expect: { timeout: 10_000 },
  reporter: "list",
  use: {
    baseURL: "http://127.0.0.1:5173",
    ...devices["Desktop Chrome"],
    viewport: { width: 1440, height: 1000 },
    launchOptions: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE
      ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE }
      : undefined,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  // These suites install isolated API fixtures. They do not build or start
  // the retired Go demo backend and do not claim full Rust runtime parity.
  testMatch: [
    "chart-stability.spec.ts",
    "node-probes.spec.ts",
    "maturity-scale.spec.ts",
    "maturity-workflows.spec.ts",
    "features.spec.ts",
  ],
  webServer: {
    command: "bun run dev -- --port 5173 --strictPort",
    url: "http://127.0.0.1:5173",
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },
});
