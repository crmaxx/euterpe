import { useEffect, useState } from "react";
import { useServerInfo, useTestLogin } from "@/api/hooks";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useToast } from "@/hooks/use-toast";
import {
  getDefaultQuality,
  QUALITY_OPTIONS,
  setDefaultQuality,
  type QualityId,
} from "@/lib/quality";

export function SettingsPage() {
  const { data: info } = useServerInfo();
  const testLogin = useTestLogin();
  const { toast } = useToast();

  const [userId, setUserId] = useState("");
  const [authToken, setAuthToken] = useState("");
  const [quality, setQuality] = useState<QualityId>(getDefaultQuality);

  useEffect(() => {
    setQuality(getDefaultQuality());
  }, []);

  const runTest = async (persist: boolean) => {
    const id = Number(userId);
    if (!userId || !authToken || Number.isNaN(id) || id < 1) {
      toast({
        title: "Validation error",
        description: "User ID and auth token are required.",
        variant: "destructive",
      });
      return;
    }
    try {
      const res = await testLogin.mutateAsync({
        user_id: id,
        auth_token: authToken,
        persist,
      });
      toast({
        title: persist ? "Credentials saved" : "Connection OK",
        description: `Membership: ${res.membership}`,
      });
    } catch (e) {
      toast({
        title: "Connection failed",
        description: e instanceof Error ? e.message : "Unknown error",
        variant: "destructive",
      });
    }
  };

  return (
    <div className="mx-auto max-w-lg space-y-8">
      <div>
        <h2 className="text-2xl font-semibold">Settings</h2>
        <p className="text-sm text-muted-foreground">
          Qobuz session from play.qobuz.com (OAuth in-app — future).
        </p>
      </div>

      <section className="space-y-4 rounded-lg border border-border bg-card p-4">
        <h3 className="font-medium">Qobuz credentials</h3>
        <div className="space-y-2">
          <Label htmlFor="user-id">User ID</Label>
          <Input
            id="user-id"
            value={userId}
            onChange={(e) => setUserId(e.target.value)}
            placeholder="localuser.id"
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="auth-token">Auth token</Label>
          <Input
            id="auth-token"
            type="password"
            value={authToken}
            onChange={(e) => setAuthToken(e.target.value)}
          />
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="secondary"
            disabled={testLogin.isPending}
            onClick={() => void runTest(false)}
          >
            Test connection
          </Button>
          <Button
            type="button"
            disabled={testLogin.isPending}
            onClick={() => void runTest(true)}
          >
            Save to server
          </Button>
        </div>
      </section>

      <section className="space-y-4 rounded-lg border border-border bg-card p-4">
        <h3 className="font-medium">Downloads</h3>
        <div className="space-y-2">
          <Label>Default quality</Label>
          <Select
            value={String(quality)}
            onValueChange={(v) => {
              const q = Number(v) as QualityId;
              setQuality(q);
              setDefaultQuality(q);
            }}
          >
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {QUALITY_OPTIONS.map((o) => (
                <SelectItem key={o.value} value={String(o.value)}>
                  {o.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-1 text-sm">
          <Label>Library path (read-only)</Label>
          <p className="rounded-md border border-border bg-background px-3 py-2 font-mono text-xs">
            {info?.library_path ?? "…"}
          </p>
        </div>
      </section>
    </div>
  );
}
