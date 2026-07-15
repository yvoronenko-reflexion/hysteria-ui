import type { Profile, StatusPayload } from "../types";

export function Sidebar({
  profiles,
  selectedId,
  status,
  onSelect,
  onNew,
  onImport,
}: {
  profiles: Profile[];
  selectedId: string | null;
  status: StatusPayload;
  onSelect: (id: string) => void;
  onNew: () => void;
  onImport: () => void;
}) {
  const activeId =
    status.state === "connected" || status.state === "connecting" ? status.profileId : null;

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <span className="app-title">Hysteria</span>
      </div>
      <div className="sidebar-actions">
        <button className="btn-ghost" onClick={onNew}>
          + New
        </button>
        <button className="btn-ghost" onClick={onImport}>
          Import URI
        </button>
      </div>
      <div className="profile-list">
        {profiles.length === 0 && <div className="profile-empty">No profiles yet.</div>}
        {profiles.map((p) => (
          <button
            key={p.id}
            className={`profile-item ${p.id === selectedId ? "selected" : ""}`}
            onClick={() => onSelect(p.id)}
          >
            <span className="profile-name">
              {p.id === activeId && (
                <span
                  className={`profile-conn-dot ${status.state}`}
                  title={status.state === "connected" ? "Connected" : "Connecting…"}
                />
              )}
              {p.name || "(unnamed)"}
            </span>
            <span className="profile-server">{p.config.server || "—"}</span>
          </button>
        ))}
      </div>
    </aside>
  );
}
