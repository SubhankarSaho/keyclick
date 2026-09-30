import type { SoundPackInfo } from '../api';

interface PackGridProps {
  packs: SoundPackInfo[];
  selected: string;
  onSelect: (id: string) => void;
}

export function PackGrid({ packs, selected, onSelect }: PackGridProps) {
  if (packs.length === 0) {
    return <p className="muted">No sound packs found. Reinstall KeyClick to restore the built-in packs.</p>;
  }
  return (
    <div className="packs" role="radiogroup" aria-label="Sound pack">
      {packs.map((p) => {
        const active = p.id === selected;
        return (
          <button
            key={p.id}
            type="button"
            role="radio"
            aria-checked={active}
            className={`pack ${active ? 'active' : ''}`}
            title={p.description}
            onClick={() => onSelect(p.id)}
          >
            <span className="pack-name">{p.name}</span>
            <span className="pack-desc">{p.description}</span>
            {active && <span className="pack-check" aria-hidden="true">✓</span>}
          </button>
        );
      })}
    </div>
  );
}
