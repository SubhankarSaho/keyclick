import type { PageProps } from '../App';
import { api } from '../api';
import { Logo } from '../components/Logo';
import { PackGrid } from '../components/PackGrid';
import { StatusBanner } from '../components/StatusBanner';
import { CheckRow, PowerToggle } from '../components/Toggle';
import { VolumeSlider } from '../components/VolumeSlider';

export default function Home({ snap, update, navigate, report, setSnap }: PageProps) {
  const s = snap.settings;

  const selectPack = async (id: string) => {
    if (id !== s.sound_pack) await update({ sound_pack: id });
    api.preview('normal').catch(report);
  };

  const fixPermission = async () => {
    try {
      setSnap(await api.requestPermission());
      await api.openPermissionSettings();
    } catch (e) {
      report(e);
    }
  };

  return (
    <main className="page">
      <header className="header">
        <div className="brand">
          <Logo />
          <span className="wordmark">KEYCLICK</span>
        </div>
        <PowerToggle on={s.enabled} onChange={(enabled) => update({ enabled })} />
      </header>
      <p className="tagline">Make typing sound satisfying.</p>

      <StatusBanner snap={snap} onFixPermission={fixPermission} report={report} />

      <section className={s.enabled ? '' : 'dimmed'}>
        <h2>Sound pack</h2>
        <PackGrid packs={snap.packs} selected={s.sound_pack} onSelect={selectPack} />
      </section>

      <section className={s.enabled ? '' : 'dimmed'}>
        <h2>Volume</h2>
        <VolumeSlider volume={s.volume} onChange={(volume) => update({ volume })} />
      </section>

      <section className={`checks ${s.enabled ? '' : 'dimmed'}`}>
        <CheckRow label="Randomize key sounds" checked={s.randomize} onChange={(randomize) => update({ randomize })} />
        <CheckRow label="Spacebar sound" checked={s.space_sound} onChange={(space_sound) => update({ space_sound })} />
        <CheckRow label="Enter sound" checked={s.enter_sound} onChange={(enter_sound) => update({ enter_sound })} />
        <CheckRow
          label="Backspace sound"
          checked={s.backspace_sound}
          onChange={(backspace_sound) => update({ backspace_sound })}
        />
      </section>

      <footer className="footer">
        <button type="button" className="link" onClick={() => navigate('settings')}>
          ⚙ Settings
        </button>
        <button type="button" className="link" onClick={() => navigate('test')}>
          ⌨ Test keyboard
        </button>
      </footer>
    </main>
  );
}
