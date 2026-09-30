import type { PageProps } from '../App';
import { PrivacyNote } from '../components/StatusBanner';
import { SwitchRow } from '../components/Toggle';
import { VolumeSlider } from '../components/VolumeSlider';

export default function SettingsPage({ snap, update, navigate }: PageProps) {
  const s = snap.settings;
  const mac = snap.platform === 'macos';
  const loginLabel = mac ? 'Launch KeyClick at Login' : 'Start KeyClick with Windows';

  return (
    <main className="page">
      <header className="header">
        <button type="button" className="link back" onClick={() => navigate('home')}>
          ← Back
        </button>
        <span className="title">Settings</span>
        <span />
      </header>

      <section className="card">
        <h2>General</h2>
        <SwitchRow label="Enable KeyClick" checked={s.enabled} onChange={(enabled) => update({ enabled })} />
        <SwitchRow
          label={loginLabel}
          hint="Off unless you turn it on. KeyClick starts quietly in the background."
          checked={s.start_on_login}
          onChange={(start_on_login) => update({ start_on_login })}
        />
        <div className="row stacked">
          <span>Volume</span>
          <VolumeSlider volume={s.volume} onChange={(volume) => update({ volume })} />
        </div>
        <label className="row">
          <span>Sound pack</span>
          <select value={s.sound_pack} onChange={(e) => update({ sound_pack: e.target.value })}>
            {snap.packs.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
      </section>

      <section className="card">
        <h2>Sound</h2>
        <SwitchRow
          label="Randomize sounds"
          hint="Varies each key press so typing sounds natural."
          checked={s.randomize}
          onChange={(randomize) => update({ randomize })}
        />
        <SwitchRow label="Spacebar sound" checked={s.space_sound} onChange={(space_sound) => update({ space_sound })} />
        <SwitchRow label="Enter sound" checked={s.enter_sound} onChange={(enter_sound) => update({ enter_sound })} />
        <SwitchRow
          label="Backspace sound"
          checked={s.backspace_sound}
          onChange={(backspace_sound) => update({ backspace_sound })}
        />
        <SwitchRow
          label="Modifier sound"
          hint="Shift, Ctrl, Alt, Cmd, Caps Lock."
          checked={s.modifier_sound}
          onChange={(modifier_sound) => update({ modifier_sound })}
        />
        <SwitchRow label="Arrow-key sound" checked={s.arrow_sound} onChange={(arrow_sound) => update({ arrow_sound })} />
        <SwitchRow
          label="Key release sound"
          hint="The softer click a real switch makes when the key comes back up."
          checked={s.release_sound}
          onChange={(release_sound) => update({ release_sound })}
        />
      </section>

      <section className="card">
        <h2>Privacy</h2>
        <PrivacyNote />
      </section>

      <section className="card about">
        <p>
          Closing this window keeps KeyClick running in the {mac ? 'menu bar' : 'system tray'}. Use{' '}
          <strong>{mac ? 'Quit KeyClick' : 'Quit'}</strong> from the {mac ? 'menu bar icon' : 'tray icon'} to exit.
        </p>
        <p className="muted">
          KeyClick {snap.version}
          {snap.audio.device ? ` · Output: ${snap.audio.device}` : ''}
        </p>
      </section>
    </main>
  );
}
