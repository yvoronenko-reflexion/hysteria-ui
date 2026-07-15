import { useEffect, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { HysteriaConfig } from "./types";
import { ServerConfigForm, ClientConfigForm } from "./components/ConfigForm";
import "./App.css";

interface ConfigInit {
  config: HysteriaConfig;
  mode: "proxy" | "tun";
}

/** Root of the standalone `config-server` / `config-client` windows (opened from
 * App.tsx, routed in main.tsx via `?view=`). The main window owns the draft:
 * this window announces itself with `config-window-ready`, receives the draft
 * over `config-init`, streams edits back over `config-change`, and asks the
 * main window to persist with `config-save`. */
export default function ConfigWindow({ kind }: { kind: "server" | "client" }) {
  const [config, setConfig] = useState<HysteriaConfig | null>(null);
  const [mode, setMode] = useState<"proxy" | "tun">("tun");

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    // Announce readiness only once the listener is registered, or the main
    // window's config-init reply can race past us and leave the window blank.
    listen<ConfigInit>("config-init", (e) => {
      setConfig(e.payload.config);
      setMode(e.payload.mode);
    }).then((f) => {
      if (cancelled) {
        f();
        return;
      }
      unlisten = f;
      emit("config-window-ready", kind);
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [kind]);

  function change(next: HysteriaConfig) {
    setConfig(next);
    emit("config-change", { kind, config: next });
  }

  async function saveAndClose() {
    if (config) await emit("config-save", { config });
    getCurrentWindow().close();
  }

  if (!config) return null;

  return (
    <div className="config-window">
      <p className="modal-note">
        {kind === "server"
          ? "Server-defined settings shared via the hysteria2:// URI — configured once per server."
          : "Local device settings (listeners, tunnel, bandwidth) — set once, kept out of the shared URI."}
      </p>
      <div className="config-window-scroll">
        {kind === "server" ? (
          <ServerConfigForm config={config} onChange={change} />
        ) : (
          <ClientConfigForm config={config} mode={mode} onChange={change} />
        )}
      </div>
      <div className="modal-actions">
        <button className="btn-ghost" onClick={() => getCurrentWindow().close()}>
          Close
        </button>
        <button className="btn-primary" onClick={saveAndClose}>
          Save
        </button>
      </div>
    </div>
  );
}
