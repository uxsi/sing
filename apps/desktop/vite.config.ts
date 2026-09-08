import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(fileURLToPath(import.meta.url));
const controllerSrc = resolve(root, "../../packages/controller/src");

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@sing/controller/validate": resolve(controllerSrc, "validate.ts"),
      "@sing/controller/clashApi": resolve(controllerSrc, "clashApi.ts"),
      "@sing/controller/subscribeCore": resolve(controllerSrc, "subscribeCore.ts"),
    },
  },
  server: { port: 5173, host: "127.0.0.1" },
  build: { outDir: "dist", emptyOutDir: true },
});
