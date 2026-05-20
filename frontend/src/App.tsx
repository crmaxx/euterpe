import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import { useServerInfo } from "@/api/hooks";
import { Toaster } from "@/components/toaster";
import { AdminLogin } from "@/features/auth/AdminLogin";
import { FavoritesPage } from "@/features/favorites/FavoritesPage";
import { SourcesPage } from "@/features/sources/SourcesPage";
import { AppLayout } from "@/features/layout/AppLayout";
import { LibraryPage } from "@/features/library/LibraryPage";
import { QueuePage } from "@/features/queue/QueuePage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { ToastStateProvider } from "@/hooks/toast-provider";
import { ADMIN_UNAUTHORIZED_EVENT, getAdminToken } from "@/lib/auth";
import { syncHawkUser } from "@/lib/hawk";
import { usePreferences } from "@/hooks/use-preferences";
import { PreferencesProvider } from "@/providers/preferences-provider";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: false, staleTime: 5_000 },
  },
});

function AppRoutes() {
  const { t } = usePreferences();
  const [authed, setAuthed] = useState(() => !!getAdminToken());
  const { data: info, isLoading } = useServerInfo();

  useEffect(() => {
    const onUnauthorized = () => setAuthed(false);
    window.addEventListener(ADMIN_UNAUTHORIZED_EVENT, onUnauthorized);
    return () =>
      window.removeEventListener(ADMIN_UNAUTHORIZED_EVENT, onUnauthorized);
  }, []);

  useEffect(() => {
    syncHawkUser(authed);
  }, [authed]);

  if (isLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center text-muted-foreground">
        {t("common.loading")}
      </div>
    );
  }

  if (info?.admin_auth_required && !authed) {
    return <AdminLogin onSuccess={() => setAuthed(true)} />;
  }

  return (
    <Routes>
      <Route element={<AppLayout />}>
        <Route index element={<Navigate to="/sources" replace />} />
        <Route path="sources" element={<SourcesPage />} />
        <Route path="favorites" element={<FavoritesPage />} />
        <Route path="queue" element={<QueuePage />} />
        <Route path="library" element={<LibraryPage />} />
        <Route path="settings" element={<SettingsPage />} />
      </Route>
    </Routes>
  );
}

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <PreferencesProvider>
        <ToastStateProvider>
          <AppRoutes />
          <Toaster />
        </ToastStateProvider>
      </PreferencesProvider>
    </QueryClientProvider>
  );
}
