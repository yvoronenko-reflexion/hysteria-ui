import { useEffect, useState } from "react";
import type { StatusPayload, TrafficPayload } from "../types";

const LABELS: Record<string, string> = {
  disconnected: "Not connected",
  connecting: "Connecting…",
  connected: "Connected",
  error: "Error",
};

const MODE_LABELS: Record<string, string> = { tun: "Tunnel", proxy: "Proxy" };

function fmtBytes(n: number): string {
  if (n < 1024) return `${Math.round(n)} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function fmtUptime(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min`;
  return `${Math.floor(m / 60)} h ${m % 60} min`;
}

/** Ticks once a second while `connectedAt` is set, so uptime stays live. */
function useUptime(connectedAt: number | null): string | null {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (connectedAt == null) return;
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
  }, [connectedAt]);
  return connectedAt == null ? null : fmtUptime(now - connectedAt);
}

function Traffic({ traffic }: { traffic: TrafficPayload }) {
  return (
    <span
      className="traffic"
      title={`Session total: ↓ ${fmtBytes(traffic.rx)} · ↑ ${fmtBytes(traffic.tx)}`}
    >
      <span className="traffic-down">↓ {fmtBytes(traffic.rxRate)}/s</span>
      <span className="traffic-up">↑ {fmtBytes(traffic.txRate)}/s</span>
    </span>
  );
}

function IpChip({ ip, loading, onRefresh }: { ip: string | null; loading: boolean; onRefresh: () => void }) {
  if (!ip && !loading) return null;
  return (
    <button className="ip-chip" title="Externally visible IP — click to refresh" onClick={onRefresh}>
      <span className="ip-label">IP</span>
      {loading ? "…" : ip}
    </button>
  );
}

/** The selected profile's connection controls, one row. While another profile
 * holds the connection (`activeElsewhere`), Connect stays available but is
 * explicit about replacing it. */
export function ConnectionBar({
  status,
  traffic,
  mode,
  onModeChange,
  onConnect,
  onDisconnect,
  disabled,
  ip,
  ipLoading,
  onRefreshIp,
  activeElsewhere,
}: {
  status: StatusPayload;
  traffic: TrafficPayload | null;
  mode: "proxy" | "tun";
  onModeChange: (m: "proxy" | "tun") => void;
  onConnect: () => void;
  onDisconnect: () => void;
  disabled: boolean;
  ip: string | null;
  ipLoading: boolean;
  onRefreshIp: () => void;
  activeElsewhere: string | null;
}) {
  // When the connection belongs to a different profile, this strip renders the
  // selected profile's (disconnected) controls; the pinned strip shows the rest.
  const own = activeElsewhere == null;
  const active = own && (status.state === "connected" || status.state === "connecting");
  const state = own ? status.state : "disconnected";
  const uptime = useUptime(own && status.state === "connected" ? status.connectedAt : null);

  return (
    <>
      <div className="connection-strip">
        <span className="strip-status">
          <span className={`status-dot ${state}`} />
          {LABELS[state] ?? state}
          {active && status.mode && (
            <span className="strip-sub">
              · {MODE_LABELS[status.mode] ?? status.mode}
              {uptime ? ` · ${uptime}` : ""}
              {status.state === "connected" && (status.connectCount ?? 0) > 1 && (
                <span
                  title={`The connection dropped and was re-established ${status.connectCount! - 1} time(s) this session`}
                >
                  {" "}
                  · ↻{status.connectCount! - 1}
                </span>
              )}
            </span>
          )}
        </span>

        {!active && (
          <div className="mode-switch" role="group" aria-label="Connection mode">
            <button className={mode === "tun" ? "active" : ""} onClick={() => onModeChange("tun")}>
              Tunnel
            </button>
            <button
              className={mode === "proxy" ? "active" : ""}
              onClick={() => onModeChange("proxy")}
            >
              Proxy
            </button>
          </div>
        )}

        {own && status.state === "connected" && traffic && <Traffic traffic={traffic} />}
        {own && <IpChip ip={ip} loading={ipLoading} onRefresh={onRefreshIp} />}

        <span className="strip-spacer" />

        {activeElsewhere != null && (
          <span className="strip-note">Replaces {activeElsewhere}</span>
        )}
        {active ? (
          <button className="btn-danger" onClick={onDisconnect}>
            {status.state === "connecting" ? "Cancel" : "Disconnect"}
          </button>
        ) : (
          <button className="btn-primary" onClick={onConnect} disabled={disabled}>
            Connect
          </button>
        )}
      </div>

      {own && status.state === "error" && status.message && (
        <div className="strip-error" title={status.message}>
          {status.message}
        </div>
      )}
    </>
  );
}

/** Always-visible summary of the live connection while a *different* profile
 * is selected: its traffic, IP, and an unambiguous Disconnect. */
export function ActiveConnectionStrip({
  status,
  traffic,
  ip,
  ipLoading,
  onRefreshIp,
  onDisconnect,
  onShow,
}: {
  status: StatusPayload;
  traffic: TrafficPayload | null;
  ip: string | null;
  ipLoading: boolean;
  onRefreshIp: () => void;
  onDisconnect: () => void;
  onShow: () => void;
}) {
  const uptime = useUptime(status.state === "connected" ? status.connectedAt : null);
  return (
    <div className="connection-strip pinned">
      <span className="strip-status">
        <span className={`status-dot ${status.state}`} />
        <button className="link-btn" onClick={onShow} title="Go to the connected profile">
          {status.profileName ?? "(unnamed)"}
        </button>
        <span className="strip-sub">
          · {status.state === "connecting" ? "connecting…" : MODE_LABELS[status.mode ?? ""] ?? status.mode}
          {uptime ? ` · ${uptime}` : ""}
        </span>
      </span>
      {status.state === "connected" && traffic && <Traffic traffic={traffic} />}
      <IpChip ip={ip} loading={ipLoading} onRefresh={onRefreshIp} />
      <span className="strip-spacer" />
      <button className="btn-danger" onClick={onDisconnect}>
        {status.state === "connecting" ? "Cancel" : "Disconnect"}
      </button>
    </div>
  );
}
