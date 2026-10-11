// PROTOTYPE: four variants of the redesigned window, switchable via
// `?variant=A..D`, crossing the two open questions in PROTOTYPE-BRIEF.md:
// image style (flat silhouettes vs line art) x Device switcher (pills row
// vs title dropdown). `?scenario=` picks the fake Device list (reloads, so
// the fake core restarts); `?theme=` forces light or dark. Only mounted
// under `npm run prototype`. Throw away.
import { useEffect, useState } from "react";
import type { ArtStyle } from "./art";
import { currentScenario } from "./fakeBackend";
import { SCENARIOS } from "./scenarios";
import { type SwitcherStyle, WindowRedesign } from "./WindowRedesign";

const VARIANTS: { key: string; name: string; art: ArtStyle; switcher: SwitcherStyle }[] = [
  { key: "A", name: "Flat image · pills row", art: "flat", switcher: "pills" },
  { key: "B", name: "Line art · title dropdown", art: "line", switcher: "dropdown" },
  { key: "C", name: "Flat image · title dropdown", art: "flat", switcher: "dropdown" },
  { key: "D", name: "Line art · pills row", art: "line", switcher: "pills" },
];

const THEMES = ["system", "light", "dark"] as const;
type Theme = (typeof THEMES)[number];

function param(name: string) {
  return new URLSearchParams(location.search).get(name);
}

function setParam(name: string, value: string) {
  const url = new URL(location.href);
  url.searchParams.set(name, value);
  history.replaceState(null, "", url);
}

export function PrototypeHost() {
  const [variantKey, setVariantKey] = useState(param("variant") ?? "A");
  const [theme, setTheme] = useState<Theme>((param("theme") as Theme) ?? "system");
  const index = Math.max(0, VARIANTS.findIndex((v) => v.key === variantKey));
  const variant = VARIANTS[index];
  const scenario = currentScenario();

  const cycle = (by: number) => {
    const next = VARIANTS[(index + by + VARIANTS.length) % VARIANTS.length].key;
    setParam("variant", next);
    setVariantKey(next);
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (target.closest("input, textarea, select, [contenteditable]")) return;
      if (e.key === "ArrowLeft") cycle(-1);
      if (e.key === "ArrowRight") cycle(1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  useEffect(() => {
    if (theme === "system") document.documentElement.removeAttribute("data-theme");
    else document.documentElement.dataset.theme = theme;
  }, [theme]);

  return (
    <div className="proto-stage">
      <div className="proto-scenario">
        <label>
          Scenario{" "}
          <select
            value={scenario.key}
            onChange={(e) => {
              setParam("scenario", e.target.value);
              location.reload();
            }}
          >
            {SCENARIOS.map((s) => (
              <option key={s.key} value={s.key}>
                {s.name}
              </option>
            ))}
          </select>
        </label>
        {scenario.tip && <p>{scenario.tip}</p>}
      </div>
      <div className="proto-frame">
        <WindowRedesign art={variant.art} switcher={variant.switcher} />
      </div>
      <div className="proto-bar">
        <button onClick={() => cycle(-1)} aria-label="Previous variant">
          ←
        </button>
        <span className="proto-bar__label">
          {variant.key} ({variant.name})
        </span>
        <button onClick={() => cycle(1)} aria-label="Next variant">
          →
        </button>
        <span className="proto-bar__divider" />
        <button
          onClick={() => {
            const next = THEMES[(THEMES.indexOf(theme) + 1) % THEMES.length];
            setParam("theme", next);
            setTheme(next);
          }}
        >
          Theme: {theme}
        </button>
      </div>
    </div>
  );
}
