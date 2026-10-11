import type { ReactElement } from "react";
import type { NoiseControlMode } from "./deviceList";

// The Noise Control Mode icons, after the Galaxy Buds app's. Each is a person
// with what they hear drawn around them, in currentColor so the selector's
// theme colours them.

/** A Noise Control Mode's icon, on a 24×24 grid. */
export function ModeIcon({ mode }: { mode: NoiseControlMode }) {
  return (
    <svg className="mode-icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
      {ICONS[mode]}
    </svg>
  );
}

/** Head and shoulders, centred on `x`, its head's top near `y` + 5.6. */
function Person({ x = 12, y = 0, scale = 1 }: { x?: number; y?: number; scale?: number }) {
  return (
    <g transform={`translate(${x} ${y}) scale(${scale})`}>
      <circle cx="0" cy="9" r="3.4" />
      <path d="M-6 19.5 C-6 15.6 -3.4 13.6 0 13.6 C3.4 13.6 6 15.6 6 19.5 Z" />
    </g>
  );
}

/** Dots on an arc over the head, from left to right: sound let in. */
const AMBIENT_DOTS = [180, 150, 120, 90, 60, 30, 0].map((degrees) => {
  const angle = (degrees * Math.PI) / 180;
  return { cx: 12 + 7.4 * Math.cos(angle), cy: 10.5 - 7.4 * Math.sin(angle) };
});

/** Bars of a waveform beside the person: sound that's listened to. */
const ADAPTIVE_BARS = [
  { x: 15.5, half: 3 },
  { x: 17.5, half: 6 },
  { x: 19.5, half: 4 },
  { x: 21.5, half: 2 },
];

const ICONS: Record<NoiseControlMode, ReactElement> = {
  off: <Person y={0.5} />,
  ambient: (
    <>
      <Person y={1.5} />
      {AMBIENT_DOTS.map(({ cx, cy }) => (
        <circle key={`${cx}`} cx={cx} cy={cy} r="0.95" />
      ))}
    </>
  ),
  adaptive: (
    <>
      <Person x={9} y={0.5} />
      {ADAPTIVE_BARS.map(({ x, half }) => (
        <rect key={x} x={x - 0.7} y={16.5 - half} width="1.4" height={half * 2} rx="0.7" />
      ))}
    </>
  ),
  noiseCancelling: (
    <>
      <circle cx="12" cy="12" r="9.6" fill="none" stroke="currentColor" strokeWidth="1.8" />
      <Person y={1.7} scale={0.82} />
    </>
  ),
};
