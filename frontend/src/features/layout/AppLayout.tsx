import {
  Folder,
  Heart,
  Link2,
  ListMusic,
  Music2,
  RefreshCw,
  Settings,
  Unlink,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { NavLink, Outlet } from "react-router-dom";
import {
  useQobuzConnection,
  useInvalidateLibraryOnDownloadComplete,
  useScanLatest,
  useScanProgressSse,
  useServerInfo,
  useSyncLatest,
} from "@/api/hooks";
import { LibraryScanProgress } from "@/features/library/LibraryScanProgress";
import { cn } from "@/lib/utils";

const nav: { to: string; label: string; icon: LucideIcon }[] = [
  { to: "/favorites", label: "Favorites", icon: Heart },
  { to: "/queue", label: "Queue", icon: ListMusic },
  { to: "/library", label: "Library", icon: Folder },
  { to: "/settings", label: "Settings", icon: Settings },
];

export function AppLayout() {
  const { data: info } = useServerInfo();
  const { data: qobuz } = useQobuzConnection();
  const { data: sync } = useSyncLatest();
  const { data: libraryScan } = useScanLatest();
  useScanProgressSse();
  useInvalidateLibraryOnDownloadComplete();

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
        <div className="mb-8 flex items-start gap-2">
          <Music2
            className="mt-0.5 size-5 shrink-0 text-muted-foreground"
            aria-hidden
          />
          <div>
            <h1 className="text-lg font-semibold tracking-tight">Euterpe</h1>
            <p className="text-xs text-muted-foreground">
              v{info?.version ?? "…"}
            </p>
          </div>
        </div>
        <nav className="flex flex-col gap-1">
          {nav.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              className={({ isActive }) =>
                cn(
                  "flex items-center gap-2 rounded-md px-3 py-2 text-sm transition-colors",
                  isActive
                    ? "bg-accent text-accent-foreground"
                    : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                )
              }
            >
              <item.icon className="size-4 shrink-0 opacity-80" aria-hidden />
              {item.label}
            </NavLink>
          ))}
        </nav>
      </aside>
      <div className="flex flex-1 flex-col">
        <header className="flex items-center justify-between border-b border-border px-6 py-3 text-sm">
          <span className="flex items-center gap-1.5 text-muted-foreground">
            <RefreshCw className="size-3.5 shrink-0 opacity-70" aria-hidden />
            {syncLabel}
            {libraryScan?.run && (
              <>
                {" · "}
                Library scan: {libraryScan.run.status}
                {libraryScan.run.status === "running" ? (
                  <>
                    {" "}
                    (
                    <LibraryScanProgress
                      compact
                      status={libraryScan.run.status}
                      filesSeen={libraryScan.run.files_seen}
                      filesProcessed={libraryScan.run.files_processed}
                      filesIndexed={libraryScan.run.files_indexed}
                      filesTotal={libraryScan.run.files_total}
                    />
                    )
                  </>
                ) : null}
              </>
            )}
          </span>
          <span
            className={cn(
              "inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-xs",
              qobuz?.connected
                ? "bg-emerald-950 text-emerald-300"
                : "bg-amber-950 text-amber-300",
            )}
          >
            {qobuz?.connected ? (
              <Link2 className="size-3 shrink-0" aria-hidden />
            ) : (
              <Unlink className="size-3 shrink-0" aria-hidden />
            )}
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
