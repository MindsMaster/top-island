import { call, on } from './invoke';
import type { BridgeStatus, LyricsData, MusicAction, MusicArtwork, MusicState } from './types';

export const musicApi = {
  poll: () => call<MusicState>('music_poll'),

  /** SMTC 变化即时推送，轮询之外的低延迟通道 */
  onState: (cb: (state: MusicState) => void) => on<MusicState>('music:state', cb),

  control: (action: MusicAction, level?: number) =>
    call<string>('music_control', { action, level: level ?? null }),

  seek: (positionMs: number) => call<boolean>('music_seek', { positionMs }),

  /** 按 hash 取当前曲目封面；hash 不匹配（已切歌）时返回 null */
  artwork: (hash: string) => call<MusicArtwork | null>('music_artwork', { hash }),

  /** 按 lyricsId 取当前曲目歌词；id 不匹配（已切歌）时返回 null */
  lyrics: (id: string) => call<LyricsData | null>('music_lyrics', { id }),

  bridgeStatus: () => call<BridgeStatus>('music_bridge_status'),
};
