import { useEffect } from "react";

type UseModalKeyboardOptions = {
  open: boolean;
  onClose: () => void;
  onConfirm?: () => void;
  confirmDisabled?: boolean;
};

function shouldIgnoreEnterConfirm(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  const tag = target.tagName;
  if (tag === "TEXTAREA" || tag === "BUTTON") {
    return true;
  }
  return target.isContentEditable;
}

export function useModalKeyboard({
  open,
  onClose,
  onConfirm,
  confirmDisabled = false,
}: UseModalKeyboardOptions) {
  useEffect(() => {
    if (!open) {
      return;
    }

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
        return;
      }

      if (e.key !== "Enter" || e.shiftKey || !onConfirm || confirmDisabled) {
        return;
      }

      if (shouldIgnoreEnterConfirm(e.target)) {
        return;
      }

      e.preventDefault();
      onConfirm();
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, onClose, onConfirm, confirmDisabled]);
}
