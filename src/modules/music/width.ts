import { currentLyric } from './store';

const MIN_WIDTH = 190;
const MAX_WIDTH = 800;
/** 歌词文本以外的固定占位：岛 padding + 封面 + 间距 */
const FIXED_WIDTH = 72;
const FONT = "500 13px 'OpenRunde', -apple-system, 'Segoe UI', Roboto, sans-serif";

let ctx: CanvasRenderingContext2D | null = null;

function textWidth(text: string): number {
  if (!ctx) ctx = document.createElement('canvas').getContext('2d');
  if (!ctx) return 0;
  ctx.font = FONT;
  return ctx.measureText(text).width;
}

/** 歌词胶囊按文本实测宽度撑开；没有歌词时用默认宽度 */
export function capsuleWidth(): number | null {
  const lyric = currentLyric.value;
  if (!lyric) return null;
  return Math.round(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, textWidth(lyric) + FIXED_WIDTH)));
}
