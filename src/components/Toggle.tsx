interface PowerToggleProps {
  on: boolean;
  onChange: (on: boolean) => void;
}

/** Master ON/OFF pill shown in the header. */
export function PowerToggle({ on, onChange }: PowerToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label="KeyClick enabled"
      className={`power ${on ? 'on' : 'off'}`}
      onClick={() => onChange(!on)}
    >
      <span className="dot" />
      {on ? 'ON' : 'OFF'}
    </button>
  );
}

interface SwitchRowProps {
  label: string;
  hint?: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}

export function SwitchRow({ label, hint, checked, disabled, onChange }: SwitchRowProps) {
  return (
    <label className={`row ${disabled ? 'disabled' : ''}`}>
      <span className="row-text">
        <span>{label}</span>
        {hint && <small>{hint}</small>}
      </span>
      <input
        type="checkbox"
        className="switch"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
    </label>
  );
}

export function CheckRow({ label, checked, onChange }: Omit<SwitchRowProps, 'hint' | 'disabled'>) {
  return (
    <label className="check">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span className="box" aria-hidden="true" />
      {label}
    </label>
  );
}
