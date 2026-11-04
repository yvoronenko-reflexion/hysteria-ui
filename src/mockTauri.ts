// Browser-only IPC mock for visual UI work: `npm run dev`, then open
// http://localhost:1420/?mock in a normal browser (no Tauri shell needed).
// Never loaded inside the real app — main.tsx imports it only when the `mock`
// query param is present. Drive state transitions from the console via
// `__mock.connect()` / `__mock.disconnect()` / `__mock.traffic(rx, tx)`.

import type { Profile, StatusPayload } from "./types";
import { emptyConfig } from "./types";

function profile(id: string, name: string, server: string): Profile {
  const config = emptyConfig();
  config.server = server;
  return { id, name, config };
}

let profiles: Profile[] = [
  profile("p1", "Hysteria-443", "203.0.113.100:443"),
  profile("p2", "Hysteria-9443", "203.0.113.100:9443"),
];

let status: StatusPayload = {
  state: "disconnected",
  mode: null,
  message: null,
  profileId: null,
  profileName: null,
  connectCount: null,
  connectedAt: null,
};

// event name -> registered listener callbacks (from plugin:event|listen)
const listeners = new Map<string, ((e: { payload: unknown }) => void)[]>();
const callbacks = new Map<number, (e: { payload: unknown }) => void>();
let nextCallback = 1;

function emit(event: string, payload: unknown) {
  for (const cb of listeners.get(event) ?? []) cb({ payload });
}

function invoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  switch (cmd) {
    case "plugin:event|listen": {
      const cb = callbacks.get(args.handler as number)!;
      const list = listeners.get(args.event as string) ?? [];
      list.push(cb);
      listeners.set(args.event as string, list);
      return Promise.resolve(nextCallback++);
    }
    case "plugin:event|unlisten":
      return Promise.resolve();
    case "list_profiles":
      return Promise.resolve(profiles);
    case "save_profile": {
      const p = args.profile as Profile;
      const i = profiles.findIndex((x) => x.id === p.id);
      if (i >= 0) profiles[i] = p;
      else profiles.push(p);
      return Promise.resolve(profiles);
    }
    case "delete_profile":
      profiles = profiles.filter((x) => x.id !== args.id);
      return Promise.resolve(profiles);
    case "connection_status":
      return Promise.resolve(status);
    case "hysteria_version":
      return Promise.resolve({ version: "v2.12.2", path: "/mock/binaries/hysteria" });
    case "privilege_status":
      return Promise.resolve(false);
    case "external_ip":
      return new Promise((r) =>
        setTimeout(() => r(status.state === "connected" ? "198.51.100.7" : "84.17.42.118"), 300),
      );
    case "connect": {
      const p = args.profile as Profile;
      const mode = args.mode as string;
      status = { ...status, state: "connecting", mode, profileId: p.id, profileName: p.name };
      emit("connection-status", status);
      return Promise.resolve();
    }
    case "disconnect":
      status = {
        state: "disconnected",
        mode: null,
        message: null,
        profileId: null,
        profileName: null,
        connectCount: null,
        connectedAt: null,
      };
      emit("connection-status", status);
      return Promise.resolve();
    case "generate_yaml":
      return Promise.resolve("# mock\nserver: example.com:443\n");
    case "export_uri":
      return Promise.resolve("hysteria2://pass@example.com:443/");
    case "import_uri":
      return Promise.resolve(profile(crypto.randomUUID(), "Imported", "imported.example.com:443"));
    default:
      return Promise.resolve(null);
  }
}

(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
  invoke,
  transformCallback(cb: (e: { payload: unknown }) => void) {
    const id = nextCallback++;
    callbacks.set(id, cb);
    return id;
  },
};

// Console helpers to walk the states a real connection would produce.
const mock = {
  connect() {
    status = {
      ...status,
      state: "connected",
      connectCount: 1,
      connectedAt: Date.now() - 12 * 60_000,
    };
    emit("connection-status", status);
    emit("hysteria-log", "2026-07-15T03:01:14-07:00\tINFO\tclient mode");
    emit(
      "hysteria-log",
      '2026-07-15T03:01:14-07:00\tINFO\tconnected to server\t{"addr": "203.0.113.100:443", "count": 1}',
    );
    emit("hysteria-log", '2026-07-15T03:01:14-07:00\tINFO\tTUN listening\t{"interface": "utun7"}');
    emit(
      "hysteria-log",
      '2026-07-15T03:01:18-07:00\tWARN\tTUN TCP error\t{"addr": "[2001::1]:54944", "error": "dial error: network is unreachable"}',
    );
    emit(
      "hysteria-log",
      '2026-07-15T03:02:31-07:00\tERROR\tTCP error\t{"addr": "x.com:443", "error": "timeout"}',
    );
    mock.traffic(95_700, 47_500);
  },
  traffic(rxRate: number, txRate: number) {
    emit("traffic-stats", { rx: 1_284_000_000, tx: 214_000_000, rxRate, txRate });
  },
  error(message: string) {
    status = { ...status, state: "error", message };
    emit("connection-status", status);
  },
};
(window as unknown as Record<string, unknown>).__mock = mock;

export {};
