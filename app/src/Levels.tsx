import type { Device, LevelRange, NoiseControlMode } from "./deviceList";
import { setAmbientLevel, setNoiseCancellingLevel } from "./deviceList";
import { usePendingSetting } from "./usePendingSetting";

/**
 * Everything the window knows about Levels. A Level belongs to exactly one
 * Noise Control Mode, and a mode can have several Levels; adding a Level to
 * the window is one entry here.
 */
const LEVELS: {
  key: string;
  mode: NoiseControlMode;
  title: string;
  /** The Level's range, or null when the Device's Model doesn't have it. */
  range: (device: Device) => LevelRange | null;
  /** The reported Level; null until the Device first reports it. */
  reported: (device: Device) => number | null;
  /** Asks the Device at this address for a Level. */
  set: (address: string, level: number) => Promise<void>;
}[] = [
  {
    key: "ambientLevel",
    mode: "ambient",
    title: "Ambient Level",
    range: (device) => device.capabilities.ambientLevel,
    reported: (device) => device.ambientLevel,
    set: setAmbientLevel,
  },
  {
    key: "noiseCancellingLevel",
    mode: "noiseCancelling",
    title: "Noise Cancelling Level",
    range: (device) => device.capabilities.noiseCancellingLevel,
    reported: (device) => device.noiseCancellingLevel,
    set: setNoiseCancellingLevel,
  },
];

/** A slider for each Level of the shown mode that the Device's Model has. */
export function Levels({
  device,
  shown,
}: {
  device: Device;
  /** The Pending mode, else the Chosen Mode. */
  shown: NoiseControlMode | null;
}) {
  return LEVELS.filter((level) => level.mode === shown).map((level) => {
    const range = level.range(device);
    return (
      range && (
        // Keyed by Level, so each keeps its own Pending value and error.
        <LevelSlider
          key={level.key}
          title={level.title}
          level={level.reported(device)}
          range={range}
          set={(asked) => level.set(device.address, asked)}
        />
      )
    );
  });
}

/** Above this many steps, notches crowd the track; it shows the value instead. */
const MAX_NOTCHES = 10;

/**
 * A Level, in the Model's own steps. A Level set here shows a dashed thumb
 * until the Device reports it.
 */
function LevelSlider({
  title,
  level,
  range,
  set,
}: {
  title: string;
  /** The reported level; null until the Device first reports it. */
  level: number | null;
  range: LevelRange;
  /** Asks the Device for a level. */
  set: (level: number) => Promise<void>;
}) {
  // Dragging across notches asks for each level in turn.
  const { shown, pending, error, request } = usePendingSetting(
    level,
    set,
    (asked) => `${title} ${asked}`,
  );
  const choose = (asked: number) => request(Math.min(range.max, Math.max(range.min, asked)));

  const steps = range.max - range.min + 1;
  const notched = steps <= MAX_NOTCHES;
  return (
    <div className="level">
      <div className="level__title">{title}</div>
      <div className="level__row">
        <button
          className="level__step"
          aria-label="Less"
          disabled={shown === null || shown <= range.min}
          onClick={() => shown !== null && choose(shown - 1)}
        >
          <svg viewBox="0 0 16 16" aria-hidden>
            <path d="M3 8H13" />
          </svg>
        </button>
        <div className="level__track" data-pending={pending !== null}>
          {notched && (
            <div className="level__notches" aria-hidden>
              {Array.from({ length: steps }, (_, i) => (
                <span key={i} className="level__notch" data-testid="notch" />
              ))}
            </div>
          )}
          <input
            type="range"
            min={range.min}
            max={range.max}
            step={1}
            value={shown ?? range.min}
            disabled={shown === null}
            aria-label={title}
            aria-busy={pending !== null}
            onChange={(e) => choose(Number(e.target.value))}
          />
        </div>
        <button
          className="level__step"
          aria-label="More"
          disabled={shown === null || shown >= range.max}
          onClick={() => shown !== null && choose(shown + 1)}
        >
          <svg viewBox="0 0 16 16" aria-hidden>
            <path d="M3 8H13M8 3V13" />
          </svg>
        </button>
        {!notched && <output className="level__value">{shown ?? "–"}</output>}
      </div>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
