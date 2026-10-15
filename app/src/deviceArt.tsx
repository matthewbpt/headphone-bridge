// The window's drawings: a flat picture per Model and the small glyphs beside
// the controls. Colours come from the theme's custom properties (and
// currentColor), so every drawing follows the light or dark theme.
import type { ComponentType, ReactElement } from "react";
import type { Labels, Model, NoiseControlMode } from "./deviceList";

/** A picture per Model. Adding a Model means adding its picture here. */
const PICTURES: Record<Model, ComponentType> = {
  galaxyBuds4Pro: Buds4Pro,
  wh1000xm5: Wh1000Xm5,
};

/**
 * The Device's picture: its Model's, named after the Model, or a generic
 * headphone for an unidentified or unsupported Model.
 */
export function DevicePicture({ model, labels }: { model: Model | null; labels: Labels }) {
  const [Drawing, name] = model
    ? [PICTURES[model], labels.models[model]]
    : [GenericHeadphones, "Headphones"];
  return (
    <svg className="device-picture" viewBox="0 0 200 150" role="img" aria-label={name}>
      <Drawing />
    </svg>
  );
}

/** A pair of Galaxy Buds4 Pro, facing each other. */
function Buds4Pro() {
  return (
    <>
      <g transform="translate(64 24) scale(1.4) rotate(-3)">
        <Earbud />
      </g>
      <g transform="translate(136 24) scale(-1.4 1.4) rotate(-3)">
        <Earbud />
      </g>
    </>
  );
}

/** One earbud, ear tip pointing left and stem down, centred on x = 0. */
function Earbud() {
  return (
    <g>
      <ellipse
        cx="-20"
        cy="25"
        rx="9"
        ry="12"
        fill="var(--art-tip)"
        transform="rotate(-20 -20 25)"
      />
      <path
        d="M-14 34 C-24 24 -18 4 0 2 C16 0 23 12 21 23 C19 33 10 38 0 38 C-6 38 -11 37 -14 34 Z"
        fill="var(--art-body)"
      />
      <path d="M6 4 C16 4 22 13 20 23 C18 30 12 35 6 36 Z" fill="var(--art-body-shade)" />
      <rect x="-8" y="12" width="16" height="58" rx="8" fill="var(--art-metal)" />
      <rect x="1" y="12" width="7" height="58" rx="3.5" fill="var(--art-metal-shade)" />
      <ellipse cx="0" cy="18" rx="6" ry="4.5" fill="var(--art-metal-shade)" />
      <ellipse cx="0" cy="18" rx="3.5" ry="2.5" fill="var(--art-grille)" />
    </g>
  );
}

/** The WH-1000XM5: a slim headband over two oval ear cups. */
function Wh1000Xm5() {
  return (
    <g>
      <path
        d="M51 86 C49 34 76 17 100 17 C124 17 151 34 149 86"
        fill="none"
        stroke="var(--art-body)"
        strokeWidth="11"
        strokeLinecap="round"
      />
      <path
        d="M57 70 C58 42 78 26 100 26 C122 26 142 42 143 70"
        fill="none"
        stroke="var(--art-body-shade)"
        strokeWidth="3"
        strokeLinecap="round"
      />
      <rect x="49" y="78" width="4" height="16" rx="2" fill="var(--art-metal)" />
      <rect x="147" y="78" width="4" height="16" rx="2" fill="var(--art-metal)" />
      <EarCup x={30} padSide="right" />
      <EarCup x={132} padSide="left" />
    </g>
  );
}

/** An XM5 ear cup, its cushion on the side facing the head. */
function EarCup({ x, padSide }: { x: number; padSide: "left" | "right" }) {
  return (
    <>
      <rect x={x} y="88" width="38" height="56" rx="19" fill="var(--art-body)" />
      <rect
        x={padSide === "right" ? x + 24 : x}
        y="92"
        width="14"
        height="48"
        rx="7"
        fill="var(--art-tip)"
      />
      <rect
        x={padSide === "right" ? x + 6 : x + 26}
        y="96"
        width="6"
        height="36"
        rx="3"
        fill="var(--art-body-shade)"
      />
    </>
  );
}

/** A plain one-tone headphone, for a Model the window has no picture of. */
function GenericHeadphones() {
  return (
    <g fill="var(--art-generic)">
      <path
        d="M58 104 L58 84 C58 50 76 34 100 34 C124 34 142 50 142 84 L142 104"
        fill="none"
        stroke="var(--art-generic)"
        strokeWidth="8"
        strokeLinecap="round"
      />
      <rect x="46" y="90" width="26" height="44" rx="10" />
      <rect x="128" y="90" width="26" height="44" rx="10" />
    </g>
  );
}

/** A small battery filled to `level`, empty while the level is unknown. */
export function BatteryGlyph({ level }: { level: number | null }) {
  const fill = level === null ? 0 : Math.max(1, (12 * level) / 100);
  return (
    <svg className="battery-glyph" viewBox="0 0 18 10" aria-hidden>
      <rect
        x="0.6"
        y="0.6"
        width="14.8"
        height="8.8"
        rx="2"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.2"
      />
      <rect x="16" y="3.2" width="1.6" height="3.6" rx="0.8" fill="currentColor" />
      <rect className="battery-glyph__fill" x="2" y="2" width={fill} height="6" rx="1" />
    </svg>
  );
}

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

/** The ⏻ power symbol, as on the Power Off button. */
export function PowerIcon() {
  return (
    <svg
      className="power-icon"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      aria-hidden
    >
      <path d="M12 3v8" />
      <path d="M6.3 6.8a8 8 0 1 0 11.4 0" />
    </svg>
  );
}
