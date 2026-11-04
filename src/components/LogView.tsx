import { useEffect, useRef } from "react";
import { ChevronIcon, TrashIcon } from "./icons";

/** Split a hysteria log line into timestamp / level / rest so levels can be
 * colorized. Falls back to plain text for lines that don't match. */
function parseLine(line: string): { ts: string; level: string; rest: string } | null {
  const m = line.match(/^(\S+\s+)?(INFO|WARN|ERROR|FATAL|DEBUG)(\s+)([\s\S]*)$/);
  if (!m) return null;
  return { ts: m[1] ?? "", level: m[2], rest: m[3] + m[4] };
}

function lineClass(level: string | undefined): string {
  if (level === "WARN") return " warn";
  if (level === "ERROR" || level === "FATAL") return " error";
  return "";
}

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
        <button
          className={`log-toggle${open ? " open" : ""}`}
          onClick={onToggle}
          title={open ? "Hide logs" : "Show logs"}
        >
          <ChevronIcon /> Logs
          {logs.length > 0 ? ` (${logs.length})` : ""}
        </button>
        {open && (
          <button className="log-clear" onClick={onClear}>
            <TrashIcon /> Clear
          </button>
        )}
      </div>
      {open && (
        <div className="log-body">
          {logs.length === 0 ? (
            <div className="log-empty">No output yet.</div>
          ) : (
            logs.map((line, i) => {
              const parsed = parseLine(line);
              return (
                <div key={i} className={`log-line${lineClass(parsed?.level)}`}>
                  {parsed ? (
                    <>
                      <span className="log-ts">{parsed.ts}</span>
                      <span className="log-level">{parsed.level}</span>
                      {parsed.rest}
                    </>
                  ) : (
                    line
                  )}
                </div>
              );
            })
          )}
          <div ref={endRef} />
        </div>
      )}
    </div>
  );
}
