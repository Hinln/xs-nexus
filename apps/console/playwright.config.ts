import { defineConfig } from "@playwright/test";

const port = parsePort(process.env.XS_CONSOLE_E2E_PORT ?? "4173");
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: "./tests",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [["line"]],
  use: {
    baseURL,
    viewport: { width: 1440, height: 900 },
    colorScheme: "light",
    reducedMotion: "reduce",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  webServer: {
    command: `npm run dev -- --host 127.0.0.1 --port ${port} --strictPort`,
    url: baseURL,
    reuseExistingServer: false,
    timeout: 30_000,
  },
});

function parsePort(raw: string): number {
  if (!/^[1-9][0-9]{0,4}$/.test(raw)) throw new Error("XS_CONSOLE_E2E_PORT is invalid");
  const value = Number(raw);
  if (value > 65_535) throw new Error("XS_CONSOLE_E2E_PORT is invalid");
  return value;
}
