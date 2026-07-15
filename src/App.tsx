import { useEffect, useRef, useState } from "react";
import { emitTo, listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import * as api from "./lib/api";
import type { HysteriaConfig, Profile, StatusPayload, TrafficPayload } from "./types";
import { newProfile } from "./types";
import { Sidebar } from "./components/Sidebar";
import { ConnectionBar } from "./components/ConnectionBar";
import { LogView } from "./components/LogView";
import "./App.css";

const MAX_LOGS = 500;
const SHOW_LOGS_KEY = "hysteria-ui.showLogs";

/** The Server/Client settings live in real OS windows (independent of the main
 * window's size), one per kind, labelled `config-server` / `config-client`. */
function closeConfigWindow(kind: "server" | "client") {
  WebviewWindow.getByLabel(`config-${kind}`)
    .then((w) => w?.close())
    .catch(() => {});
}

function closeConfigWindows() {
  closeConfigWindow("server");
  closeConfigWindow("client");
}

export default function App() {
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState<Profile | null>(null);
  const [mode, setMode] = useState<"proxy" | "tun">("tun");
  const [status, setStatus] = useState<StatusPayload>({
    state: "disconnected",
    mode: null,
    message: null,
    profileId: null,
    profileName: null,
    connectCount: null,
  });
  const [traffic, setTraffic] = useState<TrafficPayload | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [showLogs, setShowLogs] = useState(() => localStorage.getItem(SHOW_LOGS_KEY) === "1");
  const [toast, setToast] = useState<string | null>(null);
  const [modal, setModal] = useState<{
    title: string;
    body: string;
    kind: "text" | "import" | "confirm";
  } | null>(null);
  const [importText, setImportText] = useState("");
  const [privInstalled, setPrivInstalled] = useState(false);
  const toastTimer = useRef<number | null>(null);
  // Latest draft/mode for the config-window event handlers (registered once).
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const modeRef = useRef(mode);
  modeRef.current = mode;
  // Id of a "+ New" draft that exists only in memory: the profile is created on
  // the Server window's Save, so closing that window instead discards it.
  const pendingNewRef = useRef<string | null>(null);
  const nameSaveTimer = useRef<number | null>(null);

  // Initial load + event subscriptions.
  useEffect(() => {
    api.listProfiles().then(setProfiles).catch(showError);
    api.connectionStatus().then(setStatus).catch(() => {});
    api.privilegeStatus().then(setPrivInstalled).catch(() => {});

    const unStatus = listen<StatusPayload>("connection-status", (e) => setStatus(e.payload));
    const unTraffic = listen<TrafficPayload>("traffic-stats", (e) => setTraffic(e.payload));
    const unLog = listen<string>("hysteria-log", (e) =>
      setLogs((prev) => [...prev, e.payload].slice(-MAX_LOGS)),
    );
    // Settings windows (ConfigWindow.tsx): hand them the draft when they open,
    // mirror their edits into the draft, persist on their Save.
    const unReady = listen<string>("config-window-ready", (e) => {
      const d = draftRef.current;
      if (!d) return;
      emitTo(`config-${e.payload}`, "config-init", { config: d.config, mode: modeRef.current });
    });
    const unChange = listen<{ config: HysteriaConfig }>("config-change", (e) =>
      setDraft((d) => (d ? { ...d, config: e.payload.config } : d)),
    );
    const unSave = listen<{ config: HysteriaConfig }>("config-save", async (e) => {
      const d = draftRef.current;
      if (!d) return;
      // Cleared before the await so the window's `destroyed` event — which
      // follows its Save — doesn't mistake this for a discarded new profile.
      pendingNewRef.current = null;
      const updated = { ...d, config: e.payload.config };
      setDraft(updated);
      draftRef.current = updated;
      try {
        const list = await api.saveProfile(updated);
        setProfiles(list);
        setSelectedId(updated.id);
        showToast("Saved");
      } catch (err) {
        showError(err);
      }
    });
    return () => {
      unStatus.then((f) => f());
      unTraffic.then((f) => f());
      unLog.then((f) => f());
      unReady.then((f) => f());
      unChange.then((f) => f());
      unSave.then((f) => f());
    };
  }, []);

  // The client settings form depends on the mode; a stale window is confusing.
  useEffect(() => {
    closeConfigWindow("client");
  }, [mode]);

  // Stale rates are meaningless once the connection is down.
  useEffect(() => {
    if (status.state !== "connected") setTraffic(null);
  }, [status.state]);

  function toggleLogs() {
    setShowLogs((v) => {
      localStorage.setItem(SHOW_LOGS_KEY, v ? "0" : "1");
      return !v;
    });
  }

  function showToast(msg: string) {
    setToast(msg);
    if (toastTimer.current) window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 2500);
  }
  function showError(e: unknown) {
    showToast(typeof e === "string" ? e : `${e}`);
  }

  function selectProfile(id: string) {
    const p = profiles.find((x) => x.id === id);
    if (!p) return;
    pendingNewRef.current = null;
    closeConfigWindows();
    setSelectedId(id);
    setDraft(structuredClone(p));
    setMode("tun");
  }

  /** A new profile isn't persisted here — it's an in-memory draft until the
   * Server window's Save creates it, so "+ New" opens that window right away. */
  function createProfile() {
    const p = newProfile();
    pendingNewRef.current = p.id;
    setDraft(p);
    draftRef.current = p;
    setSelectedId(p.id);
    setMode("tun");
    openConfigWindow("server");
  }

  /** The main window has no Save button, so a rename has to persist itself.
   * Debounced to avoid a disk write per keystroke. */
  function renameProfile(name: string) {
    if (!draft) return;
    const updated = { ...draft, name };
    setDraft(updated);
    draftRef.current = updated;
    if (pendingNewRef.current === updated.id) return; // not created yet
    if (nameSaveTimer.current) window.clearTimeout(nameSaveTimer.current);
    nameSaveTimer.current = window.setTimeout(() => {
      api.saveProfile(updated).then(setProfiles).catch(showError);
    }, 400);
  }

  async function openConfigWindow(kind: "server" | "client") {
    const label = `config-${kind}`;
    // Only one settings window at a time: each holds a full config snapshot,
    // so concurrent windows could clobber each other's edits on save.
    closeConfigWindow(kind === "server" ? "client" : "server");
    try {
      const existing = await WebviewWindow.getByLabel(label);
      if (existing) {
        // Re-init rather than just focusing: the window may still be showing a
        // previously selected profile's config.
        const d = draftRef.current;
        if (d) await emitTo(label, "config-init", { config: d.config, mode: modeRef.current });
        await existing.setFocus();
        return;
      }
      const w = new WebviewWindow(label, {
        url: `index.html?view=${kind}`,
        title: kind === "server" ? "Server settings" : "Client settings",
        width: 500,
        height: kind === "server" ? 620 : 720,
        minWidth: 420,
        minHeight: 360,
      });
      w.once("tauri://error", (e) => showError(e.payload));
      if (kind === "server") {
        w.once("tauri://destroyed", () => {
          // Closed without saving: the profile was never created, so drop it.
          if (pendingNewRef.current && pendingNewRef.current === draftRef.current?.id) {
            pendingNewRef.current = null;
            closeConfigWindow("client");
            setDraft(null);
            setSelectedId(null);
          }
        });
      }
    } catch (e) {
      showError(e);
    }
  }

  async function remove() {
    if (!draft) return;
    setModal(null);
    pendingNewRef.current = null;
    try {
      const list = await api.deleteProfile(draft.id);
      closeConfigWindows();
      setProfiles(list);
      setDraft(null);
      setSelectedId(null);
    } catch (e) {
      showError(e);
    }
  }

  function selectProfileFrom(list: Profile[], id: string) {
    const p = list.find((x) => x.id === id);
    if (p) {
      closeConfigWindows();
      setSelectedId(id);
      setDraft(structuredClone(p));
      setMode("tun");
    }
  }

  async function doImport() {
    try {
      const p = await api.importUri(importText.trim());
      setModal(null);
      setImportText("");
      const list = await api.saveProfile(p);
      setProfiles(list);
      selectProfileFrom(list, p.id);
      showToast("Imported");
    } catch (e) {
      showError(e);
    }
  }

  async function exportUri() {
    if (!draft) return;
    try {
      const uri = await api.exportUri(draft);
      await writeText(uri);
      setModal({ title: "Share URI (copied to clipboard)", body: uri, kind: "text" });
    } catch (e) {
      showError(e);
    }
  }

  async function showYaml() {
    if (!draft) return;
    try {
      // Same mode the Connect button would use, so the preview matches what
      // actually gets written (tun/listener blocks are derived from it).
      const yaml = await api.generateYaml(draft.config, mode);
      setModal({ title: `Generated config.yaml (${mode} mode)`, body: yaml, kind: "text" });
    } catch (e) {
      showError(e);
    }
  }

  async function connect() {
    if (!draft) return;
    setLogs([]);
    try {
      // The backend derives tun.enabled (and a fallback proxy listener) from `mode`.
      await api.connect(draft, mode);
    } catch (e) {
      showError(e);
    }
  }

  async function disconnect() {
    try {
      await api.disconnect();
    } catch (e) {
      showError(e);
    }
  }

  async function setupPrivileges() {
    try {
      await api.installPrivileges();
      setPrivInstalled(true);
      showToast("Passwordless TUN enabled");
    } catch (e) {
      showError(e);
    }
  }

  async function revokePrivileges() {
    try {
      await api.uninstallPrivileges();
      setPrivInstalled(false);
      showToast("Passwordless access revoked");
    } catch (e) {
      showError(e);
    }
  }

  return (
    <div className="app">
      <Sidebar
        profiles={profiles}
        selectedId={selectedId}
        status={status}
        onSelect={selectProfile}
        onNew={createProfile}
        onImport={() => setModal({ title: "Import hysteria2:// URI", body: "", kind: "import" })}
      />

      <main className="main">
        {draft ? (
          <>
            <div className="editor-header">
              <input
                className="name-input"
                value={draft.name}
                onChange={(e) => renameProfile(e.target.value)}
                placeholder="Profile name"
              />
              <div className="editor-actions">
                <button
                  className="btn-ghost"
                  title="Server settings (address, TLS, obfuscation)"
                  onClick={() => openConfigWindow("server")}
                >
                  ⚙ Server
                </button>
                <button
                  className="btn-ghost"
                  title="Client settings (listeners, tunnel, bandwidth)"
                  onClick={() => openConfigWindow("client")}
                >
                  ⚙ Client
                </button>
                <button className="btn-ghost" onClick={showYaml}>
                  View YAML
                </button>
                <button className="btn-ghost" onClick={exportUri}>
                  Export URI
                </button>
                <button
                  className="btn-ghost btn-del"
                  onClick={() =>
                    setModal({
                      title: "Delete profile?",
                      body: `“${draft.name || "(unnamed)"}” will be permanently removed.`,
                      kind: "confirm",
                    })
                  }
                >
                  Delete
                </button>
              </div>
            </div>

            <ConnectionBar
              status={status}
              traffic={traffic}
              mode={mode}
              onModeChange={setMode}
              onConnect={connect}
              onDisconnect={disconnect}
              disabled={!draft.config.server}
            />

            {mode === "tun" && (
              <div className={`priv-banner ${privInstalled ? "ok" : ""}`}>
                {privInstalled ? (
                  <>
                    <span>✓ Passwordless TUN is set up — no password prompt on connect.</span>
                    <button className="btn-ghost" onClick={revokePrivileges}>
                      Revoke
                    </button>
                  </>
                ) : (
                  <>
                    <span>
                      TUN needs admin rights and will prompt for your password on every connect
                      and disconnect.
                    </span>
                    <button className="btn-primary" onClick={setupPrivileges}>
                      Set up passwordless access
                    </button>
                  </>
                )}
              </div>
            )}

            {!draft.config.server && (
              <div className="priv-banner">
                <span>No server configured yet — set the address and credentials first.</span>
                <button className="btn-primary" onClick={() => openConfigWindow("server")}>
                  ⚙ Server settings
                </button>
              </div>
            )}

            {!showLogs && <div className="main-spacer" />}

            <LogView logs={logs} open={showLogs} onToggle={toggleLogs} onClear={() => setLogs([])} />
          </>
        ) : (
          <div className="empty-state">
            <h2>No profile selected</h2>
            <p>Create a new profile or import a hysteria2:// URI to get started.</p>
            <div className="empty-actions">
              <button className="btn-primary" onClick={createProfile}>
                + New Profile
              </button>
              <button
                className="btn-ghost"
                onClick={() => setModal({ title: "Import hysteria2:// URI", body: "", kind: "import" })}
              >
                Import URI
              </button>
            </div>
          </div>
        )}
      </main>

      {toast && <div className="toast">{toast}</div>}

      {modal && (
        <div className="modal-overlay" onClick={() => setModal(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>{modal.title}</h3>
            {modal.kind === "confirm" ? (
              <>
                <p className="modal-note">{modal.body}</p>
                <div className="modal-actions">
                  <button className="btn-ghost" autoFocus onClick={() => setModal(null)}>
                    Cancel
                  </button>
                  <button className="btn-danger" onClick={remove}>
                    Delete
                  </button>
                </div>
              </>
            ) : modal.kind === "import" ? (
              <>
                <textarea
                  className="modal-input"
                  autoFocus
                  spellCheck={false}
                  placeholder="hysteria2://..."
                  value={importText}
                  onChange={(e) => setImportText(e.target.value)}
                />
                <div className="modal-actions">
                  <button className="btn-ghost" onClick={() => setModal(null)}>
                    Cancel
                  </button>
                  <button className="btn-primary" onClick={doImport} disabled={!importText.trim()}>
                    Import
                  </button>
                </div>
              </>
            ) : (
              <>
                <pre className="modal-pre">{modal.body}</pre>
                <div className="modal-actions">
                  <button className="btn-ghost" onClick={() => setModal(null)}>
                    Close
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
