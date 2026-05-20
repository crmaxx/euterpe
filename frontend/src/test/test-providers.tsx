import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { PreferencesProvider } from "@/providers/preferences-provider";
import { ToastStateProvider } from "@/hooks/toast-provider";

export function TestProviders({ children }: { children: ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <QueryClientProvider client={qc}>
      <PreferencesProvider>
        <ToastStateProvider>{children}</ToastStateProvider>
      </PreferencesProvider>
    </QueryClientProvider>
  );
}
