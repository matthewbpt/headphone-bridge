// PROTOTYPE: hand-drawn Device images and Noise Control Mode icons for the
// window redesign. Two image styles: flat two/three-tone silhouettes, and
// single-stroke line art. Colours come from CSS custom properties.
import type { Model, NoiseControlMode } from "../deviceList";

export type ArtStyle = "flat" | "line";

export function DeviceImage({ model, style }: { model: Model | null; style: ArtStyle }) {
  const Drawing = model === "galaxyBuds4Pro" ? Buds4Pro : model === "wh1000xm5" ? Wh1000Xm5 : Generic;
  return (
    <svg className={`device-image device-image--${style}`} viewBox="0 0 200 150" role="img" aria-hidden>
      <Drawing style={style} />
    </svg>
  );
}

/** One bud, tip pointing left, stem down, drawn around x = 0. */
function Bud({ style }: { style: ArtStyle }) {
  if (style === "line") {
    return (
      <g className="line">
        <path d="M-14 12 C-24 10 -29 20 -27 28 C-25 36 -16 38 -12 33" />
        <path d="M-12 33 C-20 26 -19 6 0 3 C14 1 21 12 19 22 C18 30 12 34 8 35" />
        <path d="M-8 18 L-8 62 C-8 70 8 70 8 62 L8 18" />
        <ellipse cx="0" cy="18" rx="8" ry="5" />
        <path d="M-4 18 L4 18" />
      </g>
    );
  }
  return (
    <g>
      <ellipse cx="-20" cy="25" rx="9" ry="12" fill="var(--art-tip)" transform="rotate(-20 -20 25)" />
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

function Buds4Pro({ style }: { style: ArtStyle }) {
  return (
    <>
      <g transform="translate(64 24) scale(1.4) rotate(-3)">
        <Bud style={style} />
      </g>
      <g transform="translate(136 24) scale(-1.4 1.4) rotate(-3)">
        <Bud style={style} />
      </g>
    </>
  );
}

function Wh1000Xm5({ style }: { style: ArtStyle }) {
  if (style === "line") {
    return (
      <g className="line">
        <path d="M46 84 C44 30 74 12 100 12 C126 12 156 30 154 84" />
        <path d="M56 84 C55 40 78 22 100 22 C122 22 145 40 144 84" />
        <path d="M51 80 L51 92 M149 80 L149 92" />
        <rect x="30" y="88" width="38" height="56" rx="19" />
        <path d="M60 96 C66 106 66 126 60 136" />
        <rect x="132" y="88" width="38" height="56" rx="19" />
        <path d="M140 96 C134 106 134 126 140 136" />
      </g>
    );
  }
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
      <rect x="30" y="88" width="38" height="56" rx="19" fill="var(--art-body)" />
      <rect x="54" y="92" width="14" height="48" rx="7" fill="var(--art-tip)" />
      <rect x="132" y="88" width="38" height="56" rx="19" fill="var(--art-body)" />
      <rect x="132" y="92" width="14" height="48" rx="7" fill="var(--art-tip)" />
      <rect x="36" y="96" width="6" height="36" rx="3" fill="var(--art-body-shade)" />
      <rect x="158" y="96" width="6" height="36" rx="3" fill="var(--art-body-shade)" />
    </g>
  );
}

function Generic({ style }: { style: ArtStyle }) {
  if (style === "line") {
    return (
      <g className="line">
        <path d="M58 104 L58 84 C58 50 76 34 100 34 C124 34 142 50 142 84 L142 104" />
        <rect x="48" y="92" width="22" height="40" rx="9" />
        <rect x="130" y="92" width="22" height="40" rx="9" />
      </g>
    );
  }
  return (
    <g>
      <path
        d="M58 104 L58 84 C58 50 76 34 100 34 C124 34 142 50 142 84 L142 104"
        fill="none"
        stroke="var(--art-generic)"
        strokeWidth="8"
        strokeLinecap="round"
      />
      <rect x="46" y="90" width="26" height="44" rx="10" fill="var(--art-generic)" />
      <rect x="128" y="90" width="26" height="44" rx="10" fill="var(--art-generic)" />
    </g>
  );
}

/** Head and shoulders, filled with currentColor. */
function Person({ x = 12, y = 0, scale = 1 }: { x?: number; y?: number; scale?: number }) {
  return (
    <g transform={`translate(${x} ${y}) scale(${scale})`}>
      <circle cx="0" cy="9" r="3.4" />
      <path d="M-6 19.5 C-6 15.6 -3.4 13.6 0 13.6 C3.4 13.6 6 15.6 6 19.5 Z" />
    </g>
  );
}

export function ModeIcon({ mode }: { mode: NoiseControlMode }) {
  return (
    <svg className="mode-icon" viewBox="0 0 24 24" aria-hidden fill="currentColor">
      {mode === "off" && <Person y={0.5} />}
      {mode === "ambient" && (
        <>
          <Person y={1.5} />
          {[-180, -150, -120, -90, -60, -30, 0].map((angle) => (
            <circle
              key={angle}
              cx={12 + 7.4 * Math.cos((angle * Math.PI) / 180)}
              cy={10.5 + 7.4 * Math.sin((angle * Math.PI) / 180)}
              r="0.95"
            />
          ))}
        </>
      )}
      {mode === "adaptive" && (
        <>
          <Person x={9} y={0.5} />
          {[
            [15.5, 3],
            [17.5, 6],
            [19.5, 4],
            [21.5, 2],
          ].map(([x, h]) => (
            <rect key={x} x={x - 0.7} y={16.5 - h} width="1.4" height={h * 2} rx="0.7" />
          ))}
        </>
      )}
      {mode === "noiseCancelling" && (
        <>
          <circle cx="12" cy="12" r="9.6" fill="none" stroke="currentColor" strokeWidth="1.8" />
          <Person y={1.7} scale={0.82} />
        </>
      )}
    </svg>
  );
}

/** A small battery, filled to the level. */
export function BatteryGlyph({ level }: { level: number | null }) {
  const width = level === null ? 0 : Math.max(1, (12 * level) / 100);
  return (
    <svg className={`battery-glyph${level !== null && level <= 20 ? " battery-glyph--low" : ""}`} viewBox="0 0 18 10" aria-hidden>
      <rect x="0.6" y="0.6" width="14.8" height="8.8" rx="2" fill="none" stroke="currentColor" strokeWidth="1.2" />
      <rect x="16" y="3.2" width="1.6" height="3.6" rx="0.8" fill="currentColor" />
      <rect className="battery-glyph__fill" x="2" y="2" width={width} height="6" rx="1" />
    </svg>
  );
}
