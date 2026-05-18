import type { ReactNode } from "react";
import { useModalKeyboard } from "@/hooks/use-modal-keyboard";
import { cn } from "@/lib/utils";

type ModalProps = {
  open: boolean;
  onClose: () => void;
  onConfirm?: () => void;
  confirmDisabled?: boolean;
  children: ReactNode;
  className?: string;
};

export function Modal({
  open,
  onClose,
  onConfirm,
  confirmDisabled,
  children,
  className,
}: ModalProps) {
  useModalKeyboard({ open, onClose, onConfirm, confirmDisabled });

  if (!open) {
    return null;
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
      role="dialog"
      aria-modal="true"
    >
      <div
        className={cn(
          "w-full max-w-md space-y-4 rounded-lg border border-border bg-card p-4",
          className,
        )}
      >
        {children}
      </div>
    </div>
  );
}
