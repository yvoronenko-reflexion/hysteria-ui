import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import ConfigWindow from "./ConfigWindow";

// The settings windows load the same bundle with `?view=server|client`.
const view = new URLSearchParams(window.location.search).get("view");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {view === "server" || view === "client" ? <ConfigWindow kind={view} /> : <App />}
  </React.StrictMode>,
);
