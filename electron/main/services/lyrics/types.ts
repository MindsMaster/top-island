import type { LyricsData } from '../../../../shared/ipc';

export interface LyricsProvider {
  name: string;
  fetch(title: string, artist: string): Promise<LyricsData | null>;
}
