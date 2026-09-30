import { invoke } from '@tauri-apps/api/core';

export type KeyCategory =
  | 'normal'
  | 'space'
  | 'enter'
  | 'backspace'
  | 'tab'
  | 'escape'
  | 'modifier'
  | 'arrow'
  | 'function'
  | 'other';

export interface Settings {
  enabled: boolean;
  volume: number;
  sound_pack: string;
  randomize: boolean;
  space_sound: boolean;
  enter_sound: boolean;
  backspace_sound: boolean;
  modifier_sound: boolean;
  arrow_sound: boolean;
  release_sound: boolean;
  start_on_login: boolean;
  onboarding_complete: boolean;
}

export interface SoundPackInfo {
  id: string;
  name: string;
  description: string;
  builtin: boolean;
}

export interface Snapshot {
  settings: Settings;
  packs: SoundPackInfo[];
  audio: { ready: boolean; device: string | null; error: string | null; pack: string | null };
  keyboard: { active: boolean; permission: 'granted' | 'denied' | 'not_required'; error: string | null };
  platform: 'windows' | 'macos' | 'other';
  version: string;
}

export const STATE_EVENT = 'state-changed';
export const NAVIGATE_EVENT = 'navigate';

export const api = {
  getState: () => invoke<Snapshot>('get_state'),
  update: (patch: Partial<Settings>) => invoke<Snapshot>('update_settings', { patch }),
  preview: (category: KeyCategory) => invoke<void>('preview_sound', { category }),
  requestPermission: () => invoke<Snapshot>('request_keyboard_permission'),
  retryKeyboard: () => invoke<Snapshot>('retry_keyboard'),
  openPermissionSettings: () => invoke<void>('open_permission_settings'),
  restartApp: () => invoke<void>('restart_app'),
  hideWindow: () => invoke<void>('hide_window'),
};

export function errorText(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : 'Something went wrong.';
}
