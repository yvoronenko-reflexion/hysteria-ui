import type { StatusPayload, TrafficPayload } from "../types";

const LABELS: Record<string, string> = {
  disconnected: "Disconnected",
  connecting: "Connecting…",
  connected: "Connected",
  error: "Error",
};

function fmtBytes(n: number): string {
  if (n < 1024) return `${Math.round(n)} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function ConnectionBar({
  status,
  traffic,
  mode,
  onModeChange,
  onConnect,
  onDisconnect,
  disabled,
}: {
  status: StatusPayload;
  traffic: TrafficPayload | null;
  mode: "proxy" | "tun";
  onModeChange: (m: "proxy" | "tun") => void;
  onConnect: () => void;
  onDisconnect: () => void;
  disabled: boolean;
}) {
  const active = status.state === "connected" || status.state === "connecting";

  return (
    <div className="connection-bar">
      <div className={`status-pill status-${status.state}`}>
        <span className="dot" />
        {LABELS[status.state] ?? status.state}
        {status.mode && active ? ` · ${status.mode.toUpperCase()}` : ""}
        {status.profileName && active ? ` · ${status.profileName}` : ""}
        {status.state === "connected" && (status.connectCount ?? 0) > 1 && (
          <span
            className="reconnects"
            title={`The connection to the server dropped and was re-established ${status.connectCount! - 1} time(s) this session`}
          >
            {` · ↻${status.connectCount! - 1}`}
          </span>
        )}
      </div>

      <div className="mode-switch" role="group" aria-label="Connection mode">
        <button
          className={mode === "tun" ? "active" : ""}
          disabled={active}
          onClick={() => onModeChange("tun")}
        >
          Tunnel
        </button>
        <button
          className={mode === "proxy" ? "active" : ""}
          disabled={active}
          onClick={() => onModeChange("proxy")}
        >
          Proxy
        </button>
      </div>

      {status.state === "connected" && traffic && (
        <span
          className="traffic"
          title={`Session total: ↓ ${fmtBytes(traffic.rx)} · ↑ ${fmtBytes(traffic.tx)}`}
        >
          <span className="traffic-down">↓ {fmtBytes(traffic.rxRate)}/s</span>
          <span className="traffic-up">↑ {fmtBytes(traffic.txRate)}/s</span>
        </span>
      )}

      {active ? (
        <button className="btn-danger" onClick={onDisconnect}>
          Disconnect
        </button>
      ) : (
        <button className="btn-primary" onClick={onConnect} disabled={disabled}>
          Connect
        </button>
      )}

      {status.state === "error" && status.message && (
        <span className="conn-error" title={status.message}>
          {status.message}
        </span>
      )}
    </div>
  );
}
