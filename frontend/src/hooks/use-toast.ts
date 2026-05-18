import { useContext } from "react";
import { ToastContext } from "@/hooks/toast-context";

export function useToast() {
  const ctx = useContext(ToastContext);
  if (!ctx) {
    throw new Error("useToast must be used within ToastStateProvider");
  }
  return ctx;
}
