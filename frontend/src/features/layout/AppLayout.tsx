import {
  Download,
  Folder,
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
  useConvertProgressSse,
  useScanProgressSse,
  useServerInfo,
  useSyncLatest,
} from "@/api/hooks";
import { LibraryScanProgress } from "@/features/library/LibraryScanProgress";
import { cn } from "@/lib/utils";
import { usePreferences } from "@/hooks/use-preferences";

const nav: { to: string; labelKey: string; icon: LucideIcon }[] = [
  { to: "/sources", labelKey: "nav.sources", icon: Download },
  { to: "/queue", labelKey: "nav.queue", icon: ListMusic },
  { to: "/library", labelKey: "nav.library", icon: Folder },
  { to: "/settings", labelKey: "nav.settings", icon: Settings },
];

export function AppLayout() {
  const { t } = usePreferences();
  const { data: info } = useServerInfo();
  const { data: qobuz } = useQobuzConnection();
  const { data: sync } = useSyncLatest();
  const { data: libraryScan } = useScanLatest();
  useScanProgressSse();
  useConvertProgressSse();
  useInvalidateLibraryOnDownloadComplete();

  const syncLabel = (() => {
    const run = sync?.run;
    if (!run) return t("layout.syncNone");
    if (run.status === "running") return t("layout.syncRunning");
    if (run.finished_at) {
      return t("layout.syncLast", { time: run.finished_at, status: run.status });
    }
    return t("layout.syncStatus", { status: run.status });
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
              {t(item.labelKey)}
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
                {t("layout.libraryScan", { status: libraryScan.run.status })}
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
                ? "border border-emerald-500/25 bg-emerald-500/10 text-emerald-800 dark:border-emerald-800 dark:bg-emerald-950 dark:text-emerald-300"
                : "border border-amber-500/25 bg-amber-500/10 text-amber-900 dark:border-amber-800 dark:bg-amber-950 dark:text-amber-300",
            )}
          >
            {qobuz?.connected ? (
              <Link2 className="size-3 shrink-0" aria-hidden />
            ) : (
              <Unlink className="size-3 shrink-0" aria-hidden />
            )}
            {qobuz?.connected
              ? qobuz.display_name
                ? t("layout.qobuzConnectedAs", { name: qobuz.display_name })
                : t("layout.qobuzConnected")
              : t("layout.qobuzNotSignedIn")}
          </span>
        </header>
        <main className="flex-1 p-6">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
