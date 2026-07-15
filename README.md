# Hysteria Client UI

A desktop GUI client for the [Hysteria2](https://v2.hysteria.network/) proxy, built with
[Tauri](https://tauri.app/) (Rust core + React/TypeScript UI).

It lets you:

- **Create and edit** Hysteria2 client configurations through a structured form (with a raw-YAML
  escape hatch for advanced options).
- **Import and export** configs as `hysteria2://` / `hy2://` share URIs.
- **Manage multiple server profiles** and switch between them.
- **Run the connection** itself — start/stop the bundled `hysteria` binary, watch live logs, and
  expose a local SOCKS5/HTTP proxy or a system-wide **TUN** tunnel.

Runs on macOS today; the proxy path is cross-platform (Windows/Linux) with little change. TUN mode
is macOS-only for now.

## Architecture

```
src/                     React + TypeScript frontend (Vite)
  types.ts               shared model types (mirrors the Rust model)
  lib/api.ts             thin wrappers over the Tauri command IPC
  components/            Sidebar, ConfigForm, ConnectionBar, LogView, fields
  App.tsx                orchestration + event subscriptions

src-tauri/src/           Rust core
  model.rs               Profile / HysteriaConfig structs (serde, camelCase over IPC)
  uri.rs                 parse_uri() / build_uri()  — source of truth for URI <-> model
  config.rs              generate_yaml()            — model -> Hysteria2 client YAML
  store.rs               profile persistence (profiles.json in the app config dir)
  runner.rs              spawn/stop hysteria, stream logs, track status; TUN elevation
  lib.rs                 Tauri commands, state, system tray
src-tauri/binaries/hysteria   the hysteria client binary (bundled as a resource)
```

The Rust side is the source of truth: URI parsing and YAML generation live in `uri.rs` / `config.rs`
and are exercised by unit tests (including a round-trip of the reference example URI). The frontend
never re-implements that logic — it calls the commands.

## Prerequisites

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://rustup.rs/) (stable)
- The `hysteria` binary at `src-tauri/binaries/hysteria`. Download the client for your platform
  from the [releases page](https://github.com/apernet/hysteria/releases/latest):

  ```sh
  # macOS Apple Silicon example
  curl -fL -o src-tauri/binaries/hysteria \
    https://github.com/apernet/hysteria/releases/latest/download/hysteria-darwin-arm64
  chmod +x src-tauri/binaries/hysteria
  ```

## Develop

```sh
npm install
npm run tauri dev
```

## Build

```sh
npm run tauri build      # produces a .app and .dmg under src-tauri/target/release/bundle/
```

## Testing

```sh
cd src-tauri && cargo test     # URI parse/build + YAML generation unit tests
```

## Connection modes

- **Proxy** (default, no privileges): runs `hysteria client -c <config>` as a normal child process
  and exposes the SOCKS5/HTTP listeners you configured. Use it with, e.g.,
  `curl --socks5 127.0.0.1:1080 https://ifconfig.me`.
- **TUN** (system-wide, requires admin): the tunnel device needs root. On macOS the interface name
  must be `utun<N>` (assigned automatically) and the server's own IP is excluded from the tunnel so
  the proxy connection isn't routed back into itself.

  By default the client is launched via `osascript ... with administrator privileges`, which prompts
  for your password on **every** connect and disconnect.

  **Grant privileges once:** in TUN mode the app shows a **"Set up passwordless access"** button. It
  installs a one-time rule under `/etc/sudoers.d/hysteria-ui` (single admin prompt) that lets *only*
  the bundled hysteria binary + this app's config path run as root without a password. After that,
  connect/disconnect never prompt. Click **Revoke** to remove the rule. The rule is pinned to the
  current binary path, so if you move the app, re-run the setup.

  > A fully prompt-free, signed `launchd` privileged helper (via `SMAppService`) is the alternative
  > production approach; it requires Developer ID signing + notarization.

## URI format

```
hysteria2://[auth@]host[:port]/?obfs=&obfs-password=&sni=&insecure=&pinSHA256=#name
```

Per the Hysteria2 spec the URI carries only connection essentials. Local settings (proxy listeners,
bandwidth, TUN, fastOpen, ...) are app-managed and are **not** part of the URI.
