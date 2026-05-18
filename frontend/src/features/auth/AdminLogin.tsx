import { useState } from "react";
import { api } from "@/api/client";
import { ApiClientError } from "@/api/errors";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { clearAdminToken, setAdminToken } from "@/lib/auth";

type Props = {
  onSuccess: () => void;
};

export function AdminLogin({ onSuccess }: Props) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const submit = async () => {
    setError(null);
    setSubmitting(true);
    setAdminToken(password);
    try {
      await api.qobuzConnection();
      onSuccess();
    } catch (e) {
      clearAdminToken();
      if (e instanceof ApiClientError && e.status === 401) {
        setError("Wrong password. Check EUTERPE_ADMIN_PASSWORD on the server.");
      } else {
        setError(e instanceof Error ? e.message : "Could not sign in");
      }
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="flex min-h-screen items-center justify-center p-6">
      <form
        className="w-full max-w-sm space-y-4 rounded-lg border border-border bg-card p-6"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <h1 className="text-xl font-semibold">Euterpe</h1>
        <p className="text-sm text-muted-foreground">
          Admin password required (EUTERPE_ADMIN_PASSWORD).
        </p>
        <div className="space-y-2">
          <Label htmlFor="admin-password">Password</Label>
          <Input
            id="admin-password"
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="current-password"
          />
        </div>
        {error && (
          <p className="text-sm text-destructive" role="alert">
            {error}
          </p>
        )}
        <Button type="submit" className="w-full" disabled={submitting}>
          {submitting ? "Signing in…" : "Sign in"}
        </Button>
      </form>
    </div>
  );
}
