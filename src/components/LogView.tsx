import { useEffect, useRef } from "react";

export function LogView({
  logs,
  open,
  onToggle,
  onClear,
}: {
  logs: string[];
  open: boolean;
  onToggle: () => void;
  onClear: () => void;
}) {
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open) endRef.current?.scrollIntoView({ block: "end" });
  }, [logs, open]);

  return (
    <div className={`log-view${open ? "" : " collapsed"}`}>
      <div className="log-header">
        <button className="log-toggle" onClick={onToggle} title={open ? "Hide logs" : "Show logs"}>
          <span className="log-chevron">{open ? "▾" : "▸"}</span> Logs
          {logs.length > 0 ? ` (${logs.length})` : ""}
        </button>
        {open && (
          <button className="btn-ghost" onClick={onClear}>
            Clear
          </button>
        )}
      </div>
      {open && (
        <div className="log-body">
          {logs.length === 0 ? (
            <div className="log-empty">No output yet.</div>
          ) : (
            logs.map((line, i) => (
              <div key={i} className="log-line">
                {line}
              </div>
            ))
          )}
          <div ref={endRef} />
        </div>
      )}
    </div>
  );
}
