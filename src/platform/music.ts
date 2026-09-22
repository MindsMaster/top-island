import { call, on } from './invoke';
import type { BridgeStatus, LyricsData, MusicAction, MusicArtwork, MusicState } from './types';

export const musicApi = {
  poll: () => call<MusicState>('music_poll'),

  /** 轮询外的即时推送 */
  onState: (cb: (state: MusicState) => void) => on<MusicState>('music:state', cb),

  control: (action: MusicAction, level?: number) =>
    call<string>('music_control', { action, level: level ?? null }),

  seek: (positionMs: number) => call<boolean>('music_seek', { positionMs }),

  /** hash 不匹配返回 null */
  artwork: (hash: string) => call<MusicArtwork | null>('music_artwork', { hash }),

  /** id 不匹配返回 null */
  lyrics: (id: string) => call<LyricsData | null>('music_lyrics', { id }),

  bridgeStatus: () => call<BridgeStatus>('music_bridge_status'),
};
