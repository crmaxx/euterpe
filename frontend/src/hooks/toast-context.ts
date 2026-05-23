import * as React from "react";

export type ToastMessage = {
  id: string;
  title: string;
  description?: string;
  variant?: "default" | "destructive";
};

export const TOAST_DURATION_MS = 5000;
export const ERROR_TOAST_DURATION_MS = 15000;

export function toastDuration(msg: Pick<ToastMessage, "variant">) {
  return msg.variant === "destructive"
    ? ERROR_TOAST_DURATION_MS
    : TOAST_DURATION_MS;
}

export type ToastContextValue = {
  toasts: ToastMessage[];
  toast: (msg: Omit<ToastMessage, "id">) => void;
  dismiss: (id: string) => void;
};

export const ToastContext = React.createContext<ToastContextValue | null>(null);
