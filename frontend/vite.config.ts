import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import pkg from "./package.json";

const devApiProxy = process.env.VITE_DEV_API_PROXY ?? "http://127.0.0.1:8080";

export default defineConfig({
  // Load VITE_* from repo root `.env` (same file as HAWK_TOKEN for the server).
  envDir: path.resolve(__dirname, ".."),
  define: {
    "import.meta.env.VITE_APP_VERSION": JSON.stringify(pkg.version),
  },
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    proxy: {
      "/api": devApiProxy,
      "/health": devApiProxy,
    },
  },
});
