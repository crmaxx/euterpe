import * as React from "react";

export type ToastMessage = {
  id: string;
  title: string;
  description?: string;
  variant?: "default" | "destructive";
};

export type ToastContextValue = {
  toasts: ToastMessage[];
  toast: (msg: Omit<ToastMessage, "id">) => void;
  dismiss: (id: string) => void;
};

export const ToastContext = React.createContext<ToastContextValue | null>(null);
