import type { HysteriaConfig } from "../types";
import { Section, TextField, NumberField, ListField, Toggle, Field } from "./fields";

/** Server-defined connection settings (travel in the hysteria2:// URI).
 * Shown in the gear-button modal — configured once, rarely touched. */
export function ServerConfigForm({
  config,
  onChange,
}: {
  config: HysteriaConfig;
  onChange: (c: HysteriaConfig) => void;
}) {
  const patch = (p: Partial<HysteriaConfig>) => onChange({ ...config, ...p });
  const tls = (p: Partial<HysteriaConfig["tls"]>) => patch({ tls: { ...config.tls, ...p } });
  const obfs = (p: Partial<HysteriaConfig["obfs"]>) => patch({ obfs: { ...config.obfs, ...p } });

  return (
    <div className="config-form">
      <Section title="Server">
        <TextField
          label="Server address"
          hint="host:port"
          value={config.server}
          placeholder="example.com:443"
          onChange={(v) => patch({ server: v })}
        />
        <TextField
          label="Auth"
          hint="password or user:pass"
          value={config.auth}
          onChange={(v) => patch({ auth: v })}
        />
      </Section>

      <Section title="TLS">
        <TextField
          label="SNI"
          hint="server name"
          value={config.tls.sni}
          placeholder="example.com"
          onChange={(v) => tls({ sni: v })}
        />
        <TextField
          label="Pin SHA-256"
          hint="certificate fingerprint"
          value={config.tls.pinSHA256}
          onChange={(v) => tls({ pinSHA256: v })}
        />
        <TextField
          label="Custom CA"
          hint="path (optional)"
          value={config.tls.ca}
          onChange={(v) => tls({ ca: v })}
        />
        <Toggle
          label="Allow insecure (skip cert verification)"
          checked={config.tls.insecure}
          onChange={(v) => tls({ insecure: v })}
        />
      </Section>

      <Section title="Obfuscation">
        <Field label="Type">
          <select
            value={config.obfs.type}
            onChange={(e) => obfs({ type: e.target.value as HysteriaConfig["obfs"]["type"] })}
          >
            <option value="none">None</option>
            <option value="salamander">Salamander</option>
            <option value="gecko">Gecko</option>
          </select>
        </Field>
        {config.obfs.type !== "none" && (
          <TextField
            label="Obfs password"
            value={config.obfs.password}
            onChange={(v) => obfs({ password: v })}
          />
        )}
        {config.obfs.type === "gecko" && (
          <>
            <NumberField
              label="Min packet size"
              hint="0 = default"
              value={config.obfs.minPacketSize}
              onChange={(v) => obfs({ minPacketSize: v })}
            />
            <NumberField
              label="Max packet size"
              hint="0 = default"
              value={config.obfs.maxPacketSize}
              onChange={(v) => obfs({ maxPacketSize: v })}
            />
          </>
        )}
      </Section>
    </div>
  );
}

/** Local, device-side settings. Only the sections relevant to the current
 * connection mode are shown. */
export function ClientConfigForm({
  config,
  onChange,
  mode,
}: {
  config: HysteriaConfig;
  onChange: (c: HysteriaConfig) => void;
  mode: "proxy" | "tun";
}) {
  const patch = (p: Partial<HysteriaConfig>) => onChange({ ...config, ...p });
  const bw = (p: Partial<HysteriaConfig["bandwidth"]>) =>
    patch({ bandwidth: { ...config.bandwidth, ...p } });
  const socks5 = (p: Partial<HysteriaConfig["socks5"]>) =>
    patch({ socks5: { ...config.socks5, ...p } });
  const http = (p: Partial<HysteriaConfig["http"]>) => patch({ http: { ...config.http, ...p } });
  const tun = (p: Partial<HysteriaConfig["tun"]>) => patch({ tun: { ...config.tun, ...p } });

  return (
    <div className="config-form">
      {mode === "proxy" ? (
        <>
          <Section title="SOCKS5 listener">
            <p className="section-note">
              If neither SOCKS5 nor HTTP is enabled, SOCKS5 on 127.0.0.1:1080 starts
              automatically.
            </p>
            <Toggle
              label="Enable SOCKS5"
              checked={config.socks5.enabled}
              onChange={(v) => socks5({ enabled: v })}
            />
            {config.socks5.enabled && (
              <>
                <TextField
                  label="Listen"
                  value={config.socks5.listen}
                  placeholder="127.0.0.1:1080"
                  onChange={(v) => socks5({ listen: v })}
                />
                <TextField
                  label="Username"
                  hint="optional"
                  value={config.socks5.username}
                  onChange={(v) => socks5({ username: v })}
                />
                <TextField
                  label="Password"
                  hint="optional"
                  value={config.socks5.password}
                  onChange={(v) => socks5({ password: v })}
                />
              </>
            )}
          </Section>

          <Section title="HTTP listener">
            <Toggle
              label="Enable HTTP"
              checked={config.http.enabled}
              onChange={(v) => http({ enabled: v })}
            />
            {config.http.enabled && (
              <>
                <TextField
                  label="Listen"
                  value={config.http.listen}
                  placeholder="127.0.0.1:8080"
                  onChange={(v) => http({ listen: v })}
                />
                <TextField
                  label="Username"
                  hint="optional"
                  value={config.http.username}
                  onChange={(v) => http({ username: v })}
                />
                <TextField
                  label="Password"
                  hint="optional"
                  value={config.http.password}
                  onChange={(v) => http({ password: v })}
                />
              </>
            )}
          </Section>
        </>
      ) : (
        <Section title="Tunnel options">
          <p className="section-note">
            Routes all traffic through the tunnel (requires admin). The server IP is excluded
            automatically to prevent a routing loop.
          </p>
          <TextField
            label="Device name"
            hint="macOS: utunN, blank = auto"
            value={config.tun.name}
            placeholder="auto (utunN)"
            onChange={(v) => tun({ name: v })}
          />
          <NumberField
            label="MTU"
            value={config.tun.mtu}
            placeholder="1500"
            onChange={(v) => tun({ mtu: v })}
          />
          <TextField
            label="Timeout"
            value={config.tun.timeout}
            placeholder="5m"
            onChange={(v) => tun({ timeout: v })}
          />
          <ListField
            label="DNS servers"
            hint="comma-separated"
            value={config.tun.dns}
            placeholder="1.1.1.1, 8.8.8.8"
            onChange={(dns) => tun({ dns })}
          />
        </Section>
      )}

      <Section title="Bandwidth">
        <TextField
          label="Up"
          hint="e.g. 100 mbps"
          value={config.bandwidth.up}
          onChange={(v) => bw({ up: v })}
        />
        <TextField
          label="Down"
          hint="e.g. 200 mbps"
          value={config.bandwidth.down}
          onChange={(v) => bw({ down: v })}
        />
      </Section>

      <Section title="Options">
        <Toggle
          label="Fast Open"
          checked={config.fastOpen}
          onChange={(v) => patch({ fastOpen: v })}
        />
        <Toggle label="Lazy start" checked={config.lazy} onChange={(v) => patch({ lazy: v })} />
      </Section>

      <Section title="Advanced (raw YAML)">
        <p className="section-note">
          Merged into the generated config, overriding the fields above. Use for quic,
          port-hopping, forwarding, etc.
        </p>
        <textarea
          className="advanced-yaml"
          value={config.advanced}
          spellCheck={false}
          placeholder={"quic:\n  maxIdleTimeout: 30s"}
          onChange={(e) => patch({ advanced: e.target.value })}
        />
      </Section>
    </div>
  );
}
