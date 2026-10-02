/**
 * Small stroke icons drawn on a 16×16 grid in `currentColor`, so a button's
 * colour states apply to them. Decorative: the button carries the label.
 */
const iconProps = {
  viewBox: "0 0 16 16",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.5,
  strokeLinecap: "round",
  strokeLinejoin: "round",
  "aria-hidden": true,
  focusable: false,
} as const;

/** Two stacked servers — the hosts (daemons and relays) this client manages. */
export function ServersIcon() {
  return (
    <svg {...iconProps}>
      <rect x="2.25" y="2.25" width="11.5" height="4.75" rx="1.25" />
      <rect x="2.25" y="9" width="11.5" height="4.75" rx="1.25" />
      <path d="M5 4.625h.01M5 11.375h.01M8.5 4.625h2.5M8.5 11.375h2.5" />
    </svg>
  );
}

/** Two adjustment sliders — settings. */
export function SlidersIcon() {
  return (
    <svg {...iconProps}>
      <path d="M2.25 5h5.5M11.75 5h2M2.25 11h2M8.25 11h5.5" />
      <circle cx="9.75" cy="5" r="1.75" />
      <circle cx="6.25" cy="11" r="1.75" />
    </svg>
  );
}
