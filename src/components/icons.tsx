// Tiny inline-SVG icon set (16px grid, stroke = currentColor) replacing the
// emoji glyphs the UI used before, which render inconsistently across systems.

import type { SVGProps } from "react";

function Svg({ children, ...props }: SVGProps<SVGSVGElement>) {
  return (
    <svg
      className="icon"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...props}
    >
      {children}
    </svg>
  );
}

/** Settings (two slider rows with knobs). */
export function SlidersIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <Svg {...props}>
      <line x1="2" y1="4.5" x2="14" y2="4.5" />
      <circle cx="10.5" cy="4.5" r="1.9" fill="currentColor" stroke="none" />
      <line x1="2" y1="11.5" x2="14" y2="11.5" />
      <circle cx="5.5" cy="11.5" r="1.9" fill="currentColor" stroke="none" />
    </Svg>
  );
}

/** View YAML. */
export function CodeIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <Svg {...props}>
      <path d="M5.5 4.5 2 8l3.5 3.5" />
      <path d="M10.5 4.5 14 8l-3.5 3.5" />
    </Svg>
  );
}

/** Export / share. */
export function ShareIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <Svg {...props}>
      <path d="M8 9.5V2.5" />
      <path d="M5.2 5 8 2.2 10.8 5" />
      <path d="M3 8.5v5h10v-5" />
    </Svg>
  );
}

export function TrashIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <Svg {...props}>
      <path d="M2.5 4.5h11" />
      <path d="M5.5 4.5V3h5v1.5" />
      <path d="M4.2 4.5l.6 9h6.4l.6-9" />
    </Svg>
  );
}

export function PlusIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <Svg {...props}>
      <path d="M8 3v10M3 8h10" />
    </Svg>
  );
}

/** Import (arrow into tray). */
export function ImportIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <Svg {...props}>
      <path d="M8 2.5v7" />
      <path d="M5.2 7 8 9.8 10.8 7" />
      <path d="M3 13h10" />
    </Svg>
  );
}

/** Disclosure chevron; rotated via CSS when open. */
export function ChevronIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <Svg {...props}>
      <path d="M6 4l4 4-4 4" />
    </Svg>
  );
}
