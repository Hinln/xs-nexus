import { defineConfig } from "@playwright/test";
import path from "node:path";

const evidenceDirectory = process.env.XS_CONSOLE_E2E_EVIDENCE_DIR;
if (!evidenceDirectory) {
  throw new Error("XS_CONSOLE_E2E_EVIDENCE_DIR is required");
}
const port = parsePort(process.env.XS_CONSOLE_E2E_PORT ?? "4173");
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: "./tests-real",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 60_000,
  expect: {
    timeout: 10_000,
  },
  outputDir: path.join(evidenceDirectory, "test-results"),
  reporter: [
    ["line"],
    ["json", { outputFile: path.join(evidenceDirectory, "results.json") }],
  ],
  use: {
    baseURL,
    viewport: { width: 1440, height: 900 },
    colorScheme: "light",
    reducedMotion: "reduce",
    trace: "off",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },
  webServer: {
    command: `npm run build && npm run preview -- --host 127.0.0.1 --port ${port} --strictPort`,
    url: baseURL,
    reuseExistingServer: false,
    timeout: 60_000,
  },
});

function parsePort(raw: string): number {
  if (!/^[1-9][0-9]{0,4}$/.test(raw)) throw new Error("XS_CONSOLE_E2E_PORT is invalid");
  const value = Number(raw);
  if (value > 65_535) throw new Error("XS_CONSOLE_E2E_PORT is invalid");
  return value;
}
