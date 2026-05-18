import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useState } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import { useServerInfo } from "@/api/hooks";
import { Toaster } from "@/components/toaster";
import { AdminLogin } from "@/features/auth/AdminLogin";
import { FavoritesPage } from "@/features/favorites/FavoritesPage";
import { AppLayout } from "@/features/layout/AppLayout";
import { LibraryPage } from "@/features/library/LibraryPage";
import { QueuePage } from "@/features/queue/QueuePage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { ToastStateProvider } from "@/hooks/toast-provider";
import { getAdminToken } from "@/lib/auth";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: false, staleTime: 5_000 },
  },
});

function AppRoutes() {
  const [authed, setAuthed] = useState(() => !!getAdminToken());
  const { data: info, isLoading } = useServerInfo();

  if (isLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center text-muted-foreground">
        Loading…
      </div>
    );
  }

  if (info?.admin_auth_required && !authed) {
    return <AdminLogin onSuccess={() => setAuthed(true)} />;
  }

  return (
    <Routes>
      <Route element={<AppLayout />}>
        <Route index element={<Navigate to="/favorites" replace />} />
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
      <ToastStateProvider>
        <AppRoutes />
        <Toaster />
      </ToastStateProvider>
    </QueryClientProvider>
  );
}
