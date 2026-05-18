import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { setAdminToken } from "@/lib/auth";

type Props = {
  onSuccess: () => void;
};

export function AdminLogin({ onSuccess }: Props) {
  const [password, setPassword] = useState("");

  return (
    <div className="flex min-h-screen items-center justify-center p-6">
      <form
        className="w-full max-w-sm space-y-4 rounded-lg border border-border bg-card p-6"
        onSubmit={(e) => {
          e.preventDefault();
          setAdminToken(password);
          onSuccess();
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
        <Button type="submit" className="w-full">
          Sign in
        </Button>
      </form>
    </div>
  );
}
