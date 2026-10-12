import type { Device, LevelRange, NoiseControlMode } from "./deviceList";
import { setAmbientLevel, setNoiseCancellingLevel } from "./deviceList";
import { usePendingSetting } from "./usePendingSetting";

/**
 * A Level, named by its field in both the Device and its Capabilities: the
 * name reads the reported value and the range.
 */
type Level = "ambientLevel" | "noiseCancellingLevel";

/**
 * Everything the window knows about Levels, in the order they show. A Level
 * belongs to exactly one Noise Control Mode, and a mode can have several
 * Levels; adding a Level to the window is one entry here.
 */
const LEVELS: Record<
  Level,
  {
    mode: NoiseControlMode;
    title: string;
    /** Asks the Device at this address for a Level. */
    set: (address: string, level: number) => Promise<void>;
  }
> = {
  ambientLevel: { mode: "ambient", title: "Ambient Level", set: setAmbientLevel },
  noiseCancellingLevel: {
    mode: "noiseCancelling",
    title: "Noise Cancelling Level",
    set: setNoiseCancellingLevel,
  },
};

/** A slider for each Level of the shown mode that the Device's Model has. */
export function Levels({
  device,
  shownMode,
}: {
  device: Device;
  /** The Pending mode, else the Chosen Mode. */
  shownMode: NoiseControlMode | null;
}) {
  return (Object.keys(LEVELS) as Level[])
    .filter((level) => LEVELS[level].mode === shownMode)
    .map((level) => {
      // Null when the Device's Model doesn't have the Level.
      const range = device.capabilities[level];
      const { title, set } = LEVELS[level];
      return (
        range && (
          // Keyed by Level, so each keeps its own Pending value and error.
          <LevelSlider
            key={level}
            title={title}
            reported={device[level]}
            range={range}
            set={(asked) => set(device.address, asked)}
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
  reported,
  range,
  set,
}: {
  title: string;
  /** The reported Level; null until the Device first reports it. */
  reported: number | null;
  range: LevelRange;
  /** Asks the Device for a Level. */
  set: (level: number) => Promise<void>;
}) {
  // Dragging across notches asks for each Level in turn.
  const { shown, pending, error, request } = usePendingSetting(
    reported,
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
