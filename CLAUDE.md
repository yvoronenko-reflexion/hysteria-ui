# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A Tauri 2 desktop GUI client for the [Hysteria2](https://v2.hysteria.network/) proxy: Rust core
(`src-tauri/`) + React 19 / TypeScript / Vite frontend (`src/`). macOS is the primary target; the
proxy path is cross-platform but TUN mode is macOS-only.

## Commands

```sh
npm install                 # install frontend deps
npm run tauri dev           # run the full app (Vite + Rust) with hot reload
npm run tauri build         # produce .app + .dmg under src-tauri/target/release/bundle/
npm run build               # frontend-only typecheck + build (tsc && vite build)

cd src-tauri && cargo test              # all Rust unit tests
cd src-tauri && cargo test uri          # run tests in uri.rs only (match by name substring)
cd src-tauri && cargo test -- --nocapture   # show println! output
```

The `hysteria` binary must exist at `src-tauri/binaries/hysteria` (bundled as a resource, not in
git) or the app can't connect. See README for the download command.

## Architecture

**The Rust side is the source of truth.** URI parsing (`uri.rs`), YAML generation (`config.rs`),
and the data model (`model.rs`) live only in Rust and are covered by unit tests. The frontend never
re-implements this logic — it calls Tauri commands via the thin wrappers in `src/lib/api.ts`. When
changing config/URI/model behavior, edit the Rust and let TypeScript call it.

**IPC contract.** Every backend function reachable from the UI is a `#[tauri::command]` registered
in `lib.rs`'s `invoke_handler!`, wrapped once in `src/lib/api.ts`, and typed in `src/types.ts`.
Adding a command means touching all three. Model structs use `#[serde(rename_all = "camelCase")]`
so Rust snake_case fields cross the IPC boundary as camelCase — `src/types.ts` must mirror that
casing exactly.

**Two channels backend → frontend:**
- Commands (request/response) — the `api.ts` calls.
- Events (push) — `connection-status` (a `StatusPayload`), `hysteria-log` (log lines), and
  `traffic-stats` (a `TrafficPayload` with session bytes + rates, sampled once a second from OS
  counters: utun interface in TUN mode, per-process `nettop` in proxy mode), emitted from
  `runner.rs` and subscribed in `App.tsx` via `listen(...)`. `App.tsx` is the orchestration
  layer holding profile/status/log state. Connection state is log-driven: the runner stays in
  `connecting` until hysteria logs `connected to server`; FATAL lines / early exit yield `error`.

**The `advanced` field is a raw-YAML escape hatch.** `HysteriaConfig` models common fields
explicitly and carries an `advanced` free-form YAML string that `config.rs` merges last into the
generated config. This keeps round-trips (import URI → edit → export/generate) from silently
dropping fields Hysteria supports but the form doesn't. Note: per spec, `hysteria2://` URIs carry
**only** connection essentials — local settings (listeners, bandwidth, TUN, fastOpen) are
app-managed and never in the URI.

**Connection modes** (`runner.rs`): *Proxy* runs `hysteria client` as a normal child process;
*TUN* needs root. TUN launches via `osascript ... with administrator privileges` (prompts every
connect/disconnect) unless the user installs the passwordless rule under `/etc/sudoers.d/hysteria-ui`
(`install_privileges`), which is pinned to the current binary + config path. On macOS the TUN
interface must be `utun<N>` and the server's own IP is added to `excludeRoutes` at connect time so
proxy packets aren't routed back into the tunnel.

**Profile persistence** (`store.rs`): profiles are saved as `profiles.json` in the Tauri app config
dir; `list`/`save`/`delete` commands return the full updated list.

## Layout

```
src-tauri/src/  lib.rs (commands, tray, state)  model.rs  uri.rs  config.rs  store.rs  runner.rs
src/            App.tsx (orchestration)  types.ts  lib/api.ts  components/ (Sidebar, ConfigForm,
                ConnectionBar, LogView, fields)
```
