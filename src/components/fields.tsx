import { useEffect, useState, type ReactNode } from "react";

export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="section">
      <h3 className="section-title">{title}</h3>
      <div className="section-body">{children}</div>
    </section>
  );
}

export function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <label className="field">
      <span className="field-label">
        {label}
        {hint && <span className="field-hint">{hint}</span>}
      </span>
      {children}
    </label>
  );
}

export function TextField({
  label,
  hint,
  value,
  onChange,
  placeholder,
  type = "text",
}: {
  label: string;
  hint?: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  type?: string;
}) {
  return (
    <Field label={label} hint={hint}>
      <input
        type={type}
        value={value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
      />
    </Field>
  );
}

const parseList = (s: string) => s.split(",").map((x) => x.trim()).filter(Boolean);

/** Comma-separated list editor. Keeps the raw text as local state so separators
 * survive typing (a controlled join/split round-trip would eat trailing commas). */
export function ListField({
  label,
  hint,
  value,
  onChange,
  placeholder,
}: {
  label: string;
  hint?: string;
  value: string[];
  onChange: (v: string[]) => void;
  placeholder?: string;
}) {
  const [text, setText] = useState(() => value.join(", "));
  // Resync only on external changes (profile switch), not our own parsed echo.
  const key = JSON.stringify(value);
  useEffect(() => {
    setText((cur) => (JSON.stringify(parseList(cur)) === key ? cur : value.join(", ")));
  }, [key]);
  return (
    <Field label={label} hint={hint}>
      <input
        type="text"
        value={text}
        placeholder={placeholder}
        onChange={(e) => {
          setText(e.target.value);
          onChange(parseList(e.target.value));
        }}
      />
    </Field>
  );
}

export function NumberField({
  label,
  hint,
  value,
  onChange,
  placeholder,
}: {
  label: string;
  hint?: string;
  value: number;
  onChange: (v: number) => void;
  placeholder?: string;
}) {
  return (
    <Field label={label} hint={hint}>
      <input
        type="number"
        value={value === 0 ? "" : value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value === "" ? 0 : Number(e.target.value))}
      />
    </Field>
  );
}

export function Toggle({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label className="toggle">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span>{label}</span>
    </label>
  );
}
