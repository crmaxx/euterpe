/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Hawk integration token (browser SDK). Empty → catcher disabled. */
  readonly VITE_HAWK_TOKEN?: string;
  /** Release id for source maps (defaults to package version via Vite define). */
  readonly VITE_HAWK_RELEASE?: string;
  /** Injected at build time from `frontend/package.json` version. */
  readonly VITE_APP_VERSION?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
