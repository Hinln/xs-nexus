import { defineConfig } from "@playwright/test";
import path from "node:path";

const evidenceDirectory = process.env.XS_CONSOLE_E2E_EVIDENCE_DIR;
if (!evidenceDirectory) {
  throw new Error("XS_CONSOLE_E2E_EVIDENCE_DIR is required");
}

export default defineConfig({
  testDir: "./tests-real",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 30_000,
  outputDir: path.join(evidenceDirectory, "test-results"),
  reporter: [
    ["line"],
    ["json", { outputFile: path.join(evidenceDirectory, "results.json") }],
  ],
  use: {
    baseURL: "http://127.0.0.1:4173",
    viewport: { width: 1440, height: 900 },
    colorScheme: "light",
    reducedMotion: "reduce",
    trace: "off",
    screenshot: "only-on-failure",
  },
  webServer: {
    command: "npm run dev -- --host 127.0.0.1 --port 4173",
    url: "http://127.0.0.1:4173",
    reuseExistingServer: false,
    timeout: 30_000,
  },
});
