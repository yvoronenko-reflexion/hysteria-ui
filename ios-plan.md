# iOS port of Hysteria UI — feasibility + implementation plan

## Context

The user asked whether an iOS version of this app is possible. Investigation showed the
current architecture is fundamentally desktop-only (it works by spawning the upstream Go
`hysteria` binary as a subprocess, with `sudo`/`osascript` for TUN privilege escalation) —
none of which iOS permits. Further research cleared the one big schedule risk (Apple's
Network Extension entitlement for a Packet Tunnel Provider is self-serve, no approval
wait), so the user asked to scope a full implementation plan. This is that plan.

**This plan is research/architecture, not a "make these edits" task.** No code should be
written yet — Phase 0 below is explicitly a standalone spike to de-risk the hardest
unknown before any of this repo's files are touched.

## What was verified about this repo

- `src-tauri/src/lib.rs:95` already has `#[cfg_attr(mobile, tauri::mobile_entry_point)]` —
  Tauri's mobile entry point is wired, but the rest of `lib.rs` is not iOS-safe: the tray
  (`TrayIconBuilder`, `on_window_event` hide-to-tray, lines 100-132) is unconditional and
  the `invoke_handler` list includes `connect`/`disconnect`/`privilege_status`/
  `install_privileges`/`uninstall_privileges`, all backed by `runner.rs`'s subprocess/
  sudoers logic (`src-tauri/src/runner.rs`) which has zero iOS equivalent.
- `tauri.conf.json` identifier is `com.yscale.hysteria-ui` — the App Group used for
  main-app ↔ extension IPC would be `group.com.yscale.hysteria-ui`.
- No mobile scaffolding exists yet (no `src-tauri/gen/apple`, no iOS config blocks).
- `model.rs`, `uri.rs`, `config.rs` are pure data/parsing/YAML-generation code with no OS
  dependency — these are the reusable core.
- Confirmed via web research: Packet Tunnel Provider is self-serve (since Nov 2016) —
  enable in Xcode Signing & Capabilities on a paid Apple Developer account, no Apple
  approval request needed. Only App Push Providers and Hotspot Helper still require
  Apple's manual sign-off, and neither is relevant here.
- Confirmed via GitHub: `apernet/hysteria`'s `core/client` package (module
  `github.com/apernet/hysteria/core/v2`) is a clean library (`Client` interface,
  `NewClient(*Config)`, no cobra/viper/OS coupling) — directly `gomobile bind`-compatible
  in principle, no fork needed for the protocol engine itself.

## Executive summary

Roughly **~30% of the codebase reuses as-is** (React UI, `model.rs`, `uri.rs`,
`config.rs`'s YAML generation, the profile JSON schema). **`runner.rs` (subprocess
spawning, sudo/osascript privilege escalation) is 0% reusable** — it has no iOS
equivalent whatsoever. The net-new work is substantial: a `gomobile`-built Go
`.xcframework` wrapping `core/client`, a new `NEPacketTunnelProvider` Xcode extension
target, an App Group-based IPC layer between the main app and the extension, and a new
Tauri iOS plugin (Swift) to bridge Rust/JS to `NETunnelProviderManager`. This is closer to
building a second app that shares a UI and data model than "porting" the existing one.

## Architecture

Three separate execution contexts replace today's single Tauri process:

```
Main App (WKWebView + Tauri/Rust + existing React UI)
   │  NETunnelProviderManager: save config / start / stop
   │  App Group shared container: profiles, active config, stats snapshot
   ▼
HysteriaTunnel extension (NEPacketTunnelProvider, new Xcode target)
   │  owns: gomobile-built Hysteria "Engine", NEPacketTunnelFlow
   ▼
Hysteria.xcframework (new: gomobile bind of core/client + a new thin Go wrapper)
```

**Config flow** (replaces today's generated-YAML-file + subprocess `-c` arg): Rust's
`config::generate_yaml` (unchanged) produces the YAML; the Tauri iOS plugin hands it to
the extension via `NETunnelProviderProtocol.providerConfiguration`, which iOS itself
persists and delivers to `startTunnel(options:)` — no shared file needed for this part.

**Status/logs** (replaces `connection-status`/`hysteria-log` events emitted directly by
`runner.rs`): `NEVPNStatusDidChangeNotification`, observed on the main-app side, maps to
the existing `StatusPayload` shape and gets re-emitted as the same `connection-status`
Tauri event — so `App.tsx`'s `listen(...)` calls **don't need to change**, only the
producer moves from Rust to Swift. Fine-grained stats (bytes tx/rx, handshake info) come
from the extension via `NEProvider.handleAppMessage` (request/response) or a periodic
App Group `UserDefaults` poll as fallback. Full line-by-line log streaming like today's
`hysteria-log` has no clean equivalent across the process boundary — scope this down to
structured stats and treat `LogView.tsx` as needing a reduced-fidelity iOS variant.

**Profiles**: `store.rs`'s load/upsert/delete logic is reusable, but the active profile's
generated YAML needs a sync step into the App Group container right before
`startVPNTunnel()`, since the extension's sandboxed process can't see Tauri's private
app-config dir.

## The one big unknown: TUN/netstack adapter

`app/internal/tun` in `apernet/hysteria` wraps a real OS `utun` device (a file
descriptor). `NEPacketTunnelProvider` on iOS doesn't give you a file descriptor — it
gives you an `NEPacketTunnelFlow` with `readPackets`/`writePackets` callbacks. Whether
bridging these two is a small adapter or a multi-week fight depends on what packet-routing
abstraction hysteria2's TUN mode uses internally (a userspace netstack like gVisor, or a
raw fd it assumes it can read/write IP packets from directly). **This must be investigated
first**, before committing to the rest of the plan, since it gates a fallback decision:

- **Option A (default)**: write a thin new Go package (`hysteriamobile`) around
  `core/client` exposing `NewEngine(configYAML, tunWriter) / Start() / Stop() / Stats()`,
  using gomobile's reverse-binding support (Go interface implemented in Swift) for the
  packet I/O boundary — same mechanism `sing-box-for-apple`'s `libbox` uses.
  `extras/obfs`/`extras/transport/udphop` (Salamander obfuscation, port hopping) have no
  OS coupling and can be pulled in directly.
- **Option B (fallback)**: if the netstack adapter proves too hard, depend on `sing-box`
  instead (it already implements hysteria2 as a client outbound and has a shipped,
  reviewed iOS TUN story via `libbox`/`sing-box-for-apple`), and have `config.rs` emit a
  sing-box-flavored JSON config for iOS builds only. This changes the project's identity
  (no longer "runs upstream hysteria2's own engine") and must be a conscious decision, not
  a silent fallback discovered mid-build.

## Phasing

**Phase 0 — de-risk standalone, no Tauri/Xcode UI work yet.**
1. Read `app/internal/tun/server.go` in full to determine the real packet-plumbing
   abstraction (resolves Option A vs. B above).
2. Write the minimal `hysteriamobile` Go wrapper; get
   `gomobile bind -target ios,iossimulator -tags with_quic` to produce a working
   `.xcframework` outside this repo (throwaway Go module).
3. Build a bare-bones single-purpose test iOS app (not this app) with one
   `NEPacketTunnelProvider` extension proving: config in → engine starts → a real TCP
   connection routed through it reaches a real hysteria2 test server and carries traffic
   both ways.
4. Decision point: fall back to Option B if step 1-3 hits a wall (netstack
   incompatibility, gomobile linker issues, or Network-Extension-specific UDP socket
   restrictions affecting QUIC).

**Phase 1 — Tauri iOS scaffolding + main app shell** (can overlap late Phase 0).
1. `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`, `npx tauri ios init`,
   verify the existing React UI actually renders usably in the WKWebView shell on
   simulator (it was built for an 800-1200px desktop window — expect real responsive-
   design work, not just "it loads").
2. Gate `runner.rs`'s tray and subprocess logic behind `cfg(not(target_os = "ios"))` in
   `lib.rs` so the crate compiles for iOS at all.
3. Stand up a Tauri iOS plugin crate skeleton (Swift side compiles, stub
   `connect`/`disconnect`/`status` invoke handlers with canned responses) to validate the
   Rust↔Swift↔JS plumbing before wiring in anything real.

**Phase 2 — wire the two together.**
1. Add the `HysteriaTunnel` extension target to the generated Xcode project, App Group
   capability on both targets, vendor in the `.xcframework` proven out in Phase 0.
2. Implement `PacketTunnelProvider.swift`, a `TunnelFlowAdapter.swift` bridging
   `packetFlow` to the Go `TunWriter` interface, and `SharedConfigStore.swift`.
3. Implement the real Tauri plugin: connect → write config → `NETunnelProviderManager`
   save+start; disconnect → stop; status → `NEVPNStatusDidChangeNotification` bridged to
   the existing `connection-status` event.
4. Re-enable profile save/sync (`store.rs` + the App Group sync step).

**Phase 3 — feature parity + polish.**
1. Stats/log surfacing to `LogView.tsx` (scoped-down, see above).
2. Reimplement route-exclusion/DNS/MTU as `NEPacketTunnelNetworkSettings` construction in
   Swift, matching `config.rs`'s `tun` block semantics (today's `excludeRoutes`-for-
   server-IP logic in `runner.rs` needs a Swift equivalent here).
3. Remove the privilege-escalation UI (`privInstalled` state, "Set up passwordless
   access" banner in `App.tsx`) on iOS — no sudoers/osascript concept exists; TUN via
   extension is simply how connecting works, with no separate proxy-vs-privileged-TUN
   distinction.
4. Verify status survives the main app being backgrounded/killed while the extension
   keeps running (extension and main-app lifecycles are independent on iOS), and that the
   UI resyncs correctly on relaunch.

## File-by-file reuse

| File | iOS fate |
|---|---|
| `model.rs`, `uri.rs` | Reused unchanged — pure data/parsing, no OS dependency. |
| `config.rs` | Reused unchanged for YAML generation if Option A; needs a parallel `target_os = "ios"`-gated emitter for sing-box JSON if Option B. |
| `store.rs` | Logic reused; `profiles_path()` needs an iOS path + a new "sync active profile into App Group container" step before connect. |
| `runner.rs` | Not reusable. Gate entirely behind `cfg(not(target_os = "ios"))`; iOS gets new Swift-backed commands instead. |
| `lib.rs` | Tray/menu code needs macOS-only gating; `invoke_handler` command set diverges by platform (privilege commands don't exist on iOS). |
| `App.tsx`, `api.ts`, `types.ts`, `components/*` | Reused almost entirely — event names/shapes preserved on purpose so this layer doesn't need a rewrite; only privilege-banner logic and log-pane fidelity need conditional handling. |

## Key risks to flag

1. **TUN/netstack adapter** (see above) is the single biggest unresolved risk — gates
   Option A vs. B and should be resolved before estimating the rest of the project.
2. **QUIC-over-UDP inside a Network Extension** has historically had rough edges around
   raw UDP socket permissions in `NEPacketTunnelProvider` specifically — validate in the
   Phase 0 throwaway app, don't assume it from `sing-box-for-apple` working.
3. **Tauri has no built-in support for Network Extension targets/entitlements/App
   Groups** — every `tauri ios init` regeneration risks clobbering manual Xcode project
   surgery unless it's scripted or the generated project is hand-maintained going forward
   rather than regenerated.
4. **App Store review**: VPN-category apps get extra scrutiny, and Hysteria2's traffic
   obfuscation (designed to look like ordinary QUIC/HTTP3 to censorship infrastructure)
   can read as "evades content filtering" to a reviewer unfamiliar with the space — plan
   for clear review notes explaining the legitimate use case.
5. **Log/stats fidelity regression** versus the desktop app's live line-by-line log pane
   is a real, user-visible scope reduction — set that expectation early.
6. **Option B is a real identity change**, not a free fallback — depending on sing-box
   instead of upstream hysteria2's own client changes what "the app's core" is on iOS.

## Verification (Phase 0 spike)

Since this phase produces no changes to this repo, "done" means: a throwaway Go module
gomobile-builds into a working `.xcframework`, and a bare-bones standalone iOS test app
with a single `NEPacketTunnelProvider` extension successfully routes one real TCP
connection through a real hysteria2 test server end to end, on a physical device or
simulator with network entitlements. Only once that's proven should any of this repo's
files be touched (Phase 1 onward).
