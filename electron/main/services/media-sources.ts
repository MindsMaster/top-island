/** SMTC SourceAppUserModelId substrings that support title-based lyric search. */
const LYRIC_SUPPORTED_APPS = [
  'cloudmusic',
  'qqmusic',
  'spotify',
  'kugou',
  'kuwo',
  'itunes',
  'applemusic',
  'foobar',
  'musicbee',
  'aimp',
  'lx-music',
  'listen1',
  'yesplaymusic',
  'winamp',
];

export function lyricsSupported(sourceAppId: string): boolean {
  const id = sourceAppId.toLowerCase();
  return LYRIC_SUPPORTED_APPS.some((k) => id.includes(k));
}
