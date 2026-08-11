import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

const controllerProxy = {
  "/v1": "http://127.0.0.1:8080",
  "/health": "http://127.0.0.1:8080",
};

export default defineConfig({
  plugins: [react()],
  test: {
    include: ["src/**/*.test.ts"],
  },
  server: {
    proxy: controllerProxy,
  },
  preview: {
    proxy: controllerProxy,
  },
});
