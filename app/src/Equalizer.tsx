import { setEqualizer } from "./core";
import type { Device, Equalizer as EqualizerValue, EqualizerCapability } from "./deviceList";
import { usePendingSetting } from "./usePendingSetting";

/**
 * The Equalizer card, drawn from the Model's Capability alone: a Preset
 * dropdown listing the Model's Presets under its own names. A picked Preset
 * is Pending until the Device reports it.
 */
export function Equalizer({
  device,
  capability,
  disabled,
}: {
  device: Device;
  capability: EqualizerCapability;
  /** While the Device is Powering Off. */
  disabled: boolean;
}) {
  const { shown, pending, error, request } = usePendingSetting(
    device.equalizer,
    (asked: EqualizerValue) => setEqualizer(device.address, asked),
    (asked) => `${presetName(capability, asked.preset)} Preset`,
  );
  // A Preset the Model doesn't list, e.g. one added by a firmware update,
  // selects nothing. Neither does one not reported yet.
  const listed = shown !== null && capability.presets.some((preset) => preset.id === shown.preset);
  return (
    <fieldset className="card equalizer" disabled={disabled}>
      <legend className="equalizer__title">Equalizer</legend>
      <select
        aria-label="Preset"
        aria-busy={pending !== null}
        value={listed && shown ? shown.preset : ""}
        disabled={shown === null}
        onChange={(e) => request({ preset: Number(e.target.value), bands: null, clearBass: null })}
      >
        {!listed && (
          <option value="" disabled>
            {shown === null ? "" : "Unrecognised Preset"}
          </option>
        )}
        {capability.presets.map((preset) => (
          <option key={preset.id} value={preset.id} disabled={!preset.selectable}>
            {preset.name}
          </option>
        ))}
      </select>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
    </fieldset>
  );
}

/** The Model's name for the Preset with wire id `id`. */
function presetName(capability: EqualizerCapability, id: number): string {
  return capability.presets.find((preset) => preset.id === id)?.name ?? String(id);
}
