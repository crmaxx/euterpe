import { renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useModalKeyboard } from "@/hooks/use-modal-keyboard";

describe("useModalKeyboard", () => {
  it("calls onClose on Escape", () => {
    const onClose = vi.fn();
    renderHook(() => useModalKeyboard({ open: true, onClose }));
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("calls onConfirm on Enter when not disabled", () => {
    const onClose = vi.fn();
    const onConfirm = vi.fn();
    renderHook(() =>
      useModalKeyboard({ open: true, onClose, onConfirm }),
    );
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    expect(onConfirm).toHaveBeenCalledOnce();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("skips Enter on textarea", () => {
    const onConfirm = vi.fn();
    renderHook(() =>
      useModalKeyboard({
        open: true,
        onClose: vi.fn(),
        onConfirm,
      }),
    );
    const textarea = document.createElement("textarea");
    document.body.appendChild(textarea);
    const event = new KeyboardEvent("keydown", { key: "Enter", bubbles: true });
    Object.defineProperty(event, "target", { value: textarea });
    window.dispatchEvent(event);
    expect(onConfirm).not.toHaveBeenCalled();
    textarea.remove();
  });
});
