import { NavLink, Outlet } from "react-router-dom";
import { useQobuzConnection, useScanLatest, useServerInfo, useSyncLatest } from "@/api/hooks";
import { cn } from "@/lib/utils";

const nav = [
  { to: "/favorites", label: "Favorites" },
  { to: "/queue", label: "Queue" },
  { to: "/library", label: "Library" },
  { to: "/settings", label: "Settings" },
] as const;

export function AppLayout() {
  const { data: info } = useServerInfo();
  const { data: qobuz } = useQobuzConnection();
  const { data: sync } = useSyncLatest();
  const { data: libraryScan } = useScanLatest();

  const syncLabel = (() => {
    const run = sync?.run;
    if (!run) return "No sync yet";
    if (run.status === "running") return "Sync running…";
    if (run.finished_at) {
      return `Last sync: ${run.finished_at} (${run.status})`;
    }
    return `Sync ${run.status}`;
  })();

  return (
    <div className="flex min-h-screen">
      <aside className="w-52 border-r border-border bg-card p-4">
        <div className="mb-8">
          <h1 className="text-lg font-semibold tracking-tight">Euterpe</h1>
          <p className="text-xs text-muted-foreground">v{info?.version ?? "…"}</p>
        </div>
        <nav className="flex flex-col gap-1">
          {nav.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              className={({ isActive }) =>
                cn(
                  "rounded-md px-3 py-2 text-sm transition-colors",
                  isActive
                    ? "bg-accent text-accent-foreground"
                    : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                )
              }
            >
              {item.label}
            </NavLink>
          ))}
        </nav>
      </aside>
      <div className="flex flex-1 flex-col">
        <header className="flex items-center justify-between border-b border-border px-6 py-3 text-sm">
          <span className="text-muted-foreground">
            {syncLabel}
            {libraryScan?.run && (
              <>
                {" · "}
                Library scan: {libraryScan.run.status}
                {libraryScan.run.status === "running"
                  ? ` (${libraryScan.run.files_indexed}/${libraryScan.run.files_seen})`
                  : ""}
              </>
            )}
          </span>
          <span
            className={cn(
              "rounded-full px-2 py-0.5 text-xs",
              qobuz?.connected
                ? "bg-emerald-950 text-emerald-300"
                : "bg-amber-950 text-amber-300",
            )}
          >
            {qobuz?.connected
              ? qobuz.display_name
                ? `Qobuz: ${qobuz.display_name}`
                : "Qobuz connected"
              : "Qobuz not signed in"}
          </span>
        </header>
        <main className="flex-1 p-6">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
