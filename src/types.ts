// Mirrors the Rust model in src-tauri/src/model.rs (camelCase over IPC).

export interface Tls {
  sni: string;
  insecure: boolean;
  pinSHA256: string;
  ca: string;
}

export interface Obfs {
  type: "none" | "salamander" | "gecko";
  password: string;
  minPacketSize: number;
  maxPacketSize: number;
}

export interface Bandwidth {
  up: string;
  down: string;
}

export interface Socks5 {
  enabled: boolean;
  listen: string;
  username: string;
  password: string;
}

export interface Http {
  enabled: boolean;
  listen: string;
  username: string;
  password: string;
}

export interface Tun {
  enabled: boolean;
  name: string;
  mtu: number;
  timeout: string;
  dns: string[];
  excludeRoutes: string[];
}

export interface HysteriaConfig {
  server: string;
  auth: string;
  tls: Tls;
  obfs: Obfs;
  bandwidth: Bandwidth;
  socks5: Socks5;
  http: Http;
  tun: Tun;
  fastOpen: boolean;
  lazy: boolean;
  advanced: string;
}

export interface Profile {
  id: string;
  name: string;
  config: HysteriaConfig;
}

export type ConnState = "disconnected" | "connecting" | "connected" | "error";

export interface StatusPayload {
  state: ConnState;
  mode: string | null;
  message: string | null;
  profileId: string | null;
  profileName: string | null;
  /** Times the QUIC connection has been (re)established; 2+ means it dropped and recovered. */
  connectCount: number | null;
  /** Epoch millis of when the session first connected (survives silent redials). */
  connectedAt: number | null;
}

/** Which hysteria binary the app runs (`hysteria_version` command). */
export interface VersionInfo {
  version: string;
  path: string;
}

/** Session traffic counters pushed via the `traffic-stats` event (bytes / bytes per second). */
export interface TrafficPayload {
  rx: number;
  tx: number;
  rxRate: number;
  txRate: number;
}

export function emptyConfig(): HysteriaConfig {
  return {
    server: "",
    auth: "",
    tls: { sni: "", insecure: false, pinSHA256: "", ca: "" },
    obfs: { type: "none", password: "", minPacketSize: 0, maxPacketSize: 0 },
    bandwidth: { up: "", down: "" },
    socks5: { enabled: true, listen: "127.0.0.1:1080", username: "", password: "" },
    http: { enabled: false, listen: "127.0.0.1:8080", username: "", password: "" },
    tun: {
      enabled: false,
      name: "",
      mtu: 1500,
      timeout: "5m",
      dns: ["1.1.1.1", "8.8.8.8"],
      excludeRoutes: [],
    },
    fastOpen: false,
    lazy: false,
    advanced: "",
  };
}

export function newProfile(): Profile {
  return { id: crypto.randomUUID(), name: "New Profile", config: emptyConfig() };
}
