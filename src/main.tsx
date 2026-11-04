import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import ConfigWindow from "./ConfigWindow";

// The settings windows load the same bundle with `?view=server|client`.
const params = new URLSearchParams(window.location.search);
const view = params.get("view");

// Visual dev in a plain browser: `?mock` swaps in a fake IPC layer (see
// mockTauri.ts). The dynamic import keeps it out of the real app's bundle path.
if (params.has("mock")) {
  await import("./mockTauri");
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {view === "server" || view === "client" ? <ConfigWindow kind={view} /> : <App />}
  </React.StrictMode>,
);
