import {
  Toast,
  ToastClose,
  ToastDescription,
  ToastProvider,
  ToastTitle,
  ToastViewport,
} from "@/components/ui/toast";
import { toastDuration } from "@/hooks/toast-context";
import { useToast } from "@/hooks/use-toast";

export function Toaster() {
  const { toasts, dismiss } = useToast();

  return (
    <ToastProvider>
      {toasts.map((t) => (
        <Toast
          key={t.id}
          variant={t.variant}
          duration={toastDuration(t)}
          open
          onOpenChange={(open) => !open && dismiss(t.id)}
        >
          <div className="grid gap-1">
            <ToastTitle>{t.title}</ToastTitle>
            {t.description ? (
              <ToastDescription>{t.description}</ToastDescription>
            ) : null}
          </div>
          <ToastClose />
        </Toast>
      ))}
      <ToastViewport />
    </ToastProvider>
  );
}
