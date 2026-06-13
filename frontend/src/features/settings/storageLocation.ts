import type { StorageLocationView } from "@/api/client";

export function storageLabel(location: StorageLocationView | null | undefined): string {
  if (!location) return "not configured";
  if (location.kind === "local") return `local:${location.path}`;
  const path = location.path ? `/${location.path}` : "";
  const port = location.port !== 445 ? `:${location.port}` : "";
  return `smb://${location.host}${port}/${location.share}${path}`;
}

export function formatSmbLocation(parts: {
  host: string;
  port?: number;
  share: string;
  path?: string;
}): string {
  const port = parts.port && parts.port !== 445 ? `:${parts.port}` : "";
  const path = parts.path ? `/${parts.path.replace(/^\/+/, "")}` : "";
  return `smb://${parts.host}${port}/${parts.share}${path}`;
}

export function parseSmbLocation(
  raw: string,
): Partial<{
  host: string;
  port: number;
  share: string;
  path: string;
}> {
  const value = raw.trim();
  if (!value) return {};
  if (value.startsWith("\\\\")) {
    const [host, share, ...rest] = value.replace(/^\\\\/, "").split("\\").filter(Boolean);
    return { host, share, path: rest.join("/") };
  }
  try {
    const url = new URL(value.startsWith("smb://") ? value : `smb://${value}`);
    const [share, ...rest] = url.pathname.split("/").filter(Boolean);
    return {
      host: url.hostname,
      port: url.port ? Number(url.port) : 445,
      share,
      path: rest.join("/"),
    };
  } catch {
    return {};
  }
}

export function applyShareToSmbLocation(current: string, shareName: string): string {
  const parsed = parseSmbLocation(current);
  if (!parsed.host) return current;
  return formatSmbLocation({
    host: parsed.host,
    port: parsed.port,
    share: shareName,
    path: parsed.path,
  });
}

export function activePresetId(
  library: StorageLocationView | null | undefined,
): string | null {
  if (!library) return null;
  if (library.kind === "local") return `local:${library.path}`;
  return `smb:${library.host}:${library.port}/${library.share}/${library.path}`;
}

export function joinBrowsePath(base: string, rel: string): string {
  const parts = [base, rel]
    .map((part) => part.replace(/^\/+|\/+$/g, ""))
    .filter(Boolean);
  return parts.join("/");
}

export function childBrowsePath(current: string, name: string): string {
  return joinBrowsePath(current, name);
}
