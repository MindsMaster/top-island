import { call, on } from './invoke';
import type { GrantResult, IntegrationInfo, LyricsData, MusicAction, MusicState } from './types';

export const musicApi = {
  state: () => call<MusicState>('music_state'),

  onState: (cb: (state: MusicState) => void) => on<MusicState>('music:state', cb),

  control: (action: MusicAction, level?: number) =>
    call<string>('music_control', { action, level: level ?? null }),

  seek: (positionMs: number) => call<boolean>('music_seek', { positionMs }),

  /** id 不匹配返回 null */
  lyrics: (id: string) => call<LyricsData | null>('music_lyrics', { id }),

  integrations: () => call<IntegrationInfo[]>('music_integrations'),

  /** 会弹 UAC */
  integrationGrant: (id: string) => call<GrantResult>('music_integration_grant', { id }),
};
