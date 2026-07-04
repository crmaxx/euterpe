import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, loadEnv } from "vite";
import pkg from "./package.json";

const repoRoot = path.resolve(__dirname, "..");
const usePolling = ["1", "true", "yes"].includes(
  (process.env.CHOKIDAR_USEPOLLING ?? "").toLowerCase(),
);
const pollingInterval = Number(process.env.CHOKIDAR_INTERVAL ?? 300);
const devHost = process.env.VITE_DEV_HOST ?? "127.0.0.1";

function devProxyFromBind(bind: string | undefined): string {
  const [host = "127.0.0.1", port = "8080"] = (bind ?? "127.0.0.1:8080").split(
    ":",
  );
  const browserHost = host === "0.0.0.0" || host === "::" ? "127.0.0.1" : host;
  return `http://${browserHost}:${port}`;
}

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, repoRoot, "");
  const devApiProxy =
    process.env.VITE_DEV_API_PROXY ??
    env.VITE_DEV_API_PROXY ??
    devProxyFromBind(env.EUTERPE_BIND);

  return {
    // Load VITE_* from repo root `.env` (same file as HAWK_TOKEN for the server).
    envDir: repoRoot,
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
      host: devHost,
      watch: {
        usePolling,
        interval: Number.isFinite(pollingInterval) ? pollingInterval : 300,
      },
      proxy: {
        "/api": devApiProxy,
        "/health": devApiProxy,
      },
    },
  };
});
