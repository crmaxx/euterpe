import { Component, type ErrorInfo, type ReactNode } from "react";
import { getHawk } from "@/lib/hawk";

type Props = {
  children: ReactNode;
};

type State = {
  hasError: boolean;
};

/**
 * Reports React render errors to Hawk (global handlers do not catch these).
 */
export class HawkErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false };

  static getDerivedStateFromError(): State {
    return { hasError: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    const context = info.componentStack
      ? { componentStack: info.componentStack }
      : undefined;
    getHawk()?.send(error, context);
  }

  render(): ReactNode {
    if (this.state.hasError) {
      return (
        <div className="flex min-h-screen flex-col items-center justify-center gap-3 p-6 text-center">
          <h1 className="text-lg font-semibold">Something went wrong</h1>
          <p className="max-w-md text-sm text-muted-foreground">
            The error was reported. Try reloading the page.
          </p>
          <button
            type="button"
            className="rounded-md border px-4 py-2 text-sm hover:bg-muted"
            onClick={() => window.location.reload()}
          >
            Reload
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
