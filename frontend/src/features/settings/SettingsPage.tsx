import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import {
  useQobuzConnection,
  useQobuzLogout,
  useQobuzOAuthStart,
  useServerInfo,
} from "@/api/hooks";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { IntegrationsSection } from "@/features/settings/IntegrationsSection";
import { useToast } from "@/hooks/use-toast";
import {
  getDefaultQuality,
  QUALITY_OPTIONS,
  setDefaultQuality,
  type QualityId,
} from "@/lib/quality";

function qobuzUserLabel(connection: {
  display_name?: string | null;
  qobuz_user_id?: number | null;
}): string {
  if (connection.display_name?.trim()) {
    return connection.display_name.trim();
  }
  if (connection.qobuz_user_id != null) {
    return `User #${connection.qobuz_user_id}`;
  }
  return "Qobuz account";
}

export function SettingsPage() {
  const { data: info } = useServerInfo();
  const { data: connection, refetch: refetchConnection } = useQobuzConnection();
  const oauthStart = useQobuzOAuthStart();
  const logout = useQobuzLogout();
  const { toast } = useToast();
  const [searchParams, setSearchParams] = useSearchParams();

  const [quality, setQuality] = useState<QualityId>(getDefaultQuality);

  useEffect(() => {
    const qobuz = searchParams.get("qobuz");
    if (qobuz === "connected") {
      toast({
        title: "Qobuz connected",
        description: "Your account is linked. You can sync favorites and download.",
      });
      void refetchConnection();
      searchParams.delete("qobuz");
      searchParams.delete("account_id");
      setSearchParams(searchParams, { replace: true });
    } else if (qobuz === "error") {
      toast({
        title: "Qobuz connection failed",
        description: "Try connecting again from this page.",
        variant: "destructive",
      });
      searchParams.delete("qobuz");
      setSearchParams(searchParams, { replace: true });
    }
  }, [searchParams, setSearchParams, toast, refetchConnection]);

  const connectQobuz = async () => {
    if (!connection?.master_key_configured) {
      toast({
        title: "Server not ready",
        description:
          "EUTERPE_MASTER_KEY must be set on the server before linking Qobuz.",
        variant: "destructive",
      });
      return;
    }
    try {
      const { authorize_url } = await oauthStart.mutateAsync();
      window.location.href = authorize_url;
    } catch (e) {
      toast({
        title: "Could not start OAuth",
        description: e instanceof Error ? e.message : "Unknown error",
        variant: "destructive",
      });
    }
  };

  const logOutQobuz = async () => {
    try {
      await logout.mutateAsync();
      toast({
        title: "Signed out of Qobuz",
        description: "You can connect another account anytime.",
      });
    } catch (e) {
      toast({
        title: "Could not sign out",
        description: e instanceof Error ? e.message : "Unknown error",
        variant: "destructive",
      });
    }
  };

  const connected = connection?.connected === true;

  return (
    <div className="mx-auto max-w-3xl space-y-8">
      <div>
        <h2 className="text-2xl font-semibold">Settings</h2>
        <p className="text-sm text-muted-foreground">
          Link your Qobuz account via OAuth. Credentials stay encrypted on the
          server.
        </p>
      </div>

      <section className="space-y-4 rounded-lg border border-border bg-card p-4">
        <h3 className="font-medium">Qobuz account</h3>
        {connected && connection ? (
          <div className="space-y-3">
            <div className="space-y-1">
              <p className="text-sm font-medium text-foreground">
                {qobuzUserLabel(connection)}
              </p>
              {connection.membership_label && (
                <p className="text-sm text-muted-foreground">
                  {connection.membership_label}
                </p>
              )}
            </div>
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                variant="outline"
                disabled={oauthStart.isPending}
                onClick={() => void connectQobuz()}
              >
                Switch account
              </Button>
              <Button
                type="button"
                variant="destructive"
                disabled={logout.isPending}
                onClick={() => void logOutQobuz()}
              >
                Log out
              </Button>
            </div>
          </div>
        ) : (
          <div className="space-y-3">
            <p className="text-sm text-muted-foreground">Not signed in</p>
            <Button
              type="button"
              disabled={oauthStart.isPending}
              onClick={() => void connectQobuz()}
            >
              Connect Qobuz
            </Button>
          </div>
        )}
      </section>

      <IntegrationsSection />

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