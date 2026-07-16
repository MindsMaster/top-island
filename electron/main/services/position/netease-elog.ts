import * as fs from 'fs';
import * as path from 'path';
import type { PositionProvider, PositionInfo } from './types';

/**
 * 网易云 elog 实时进度源 网易云桌面版会把播放生命周期
 * 写入 %LOCALAPPDATA%/NetEase/CloudMusic/cloudmusic.elog
 * 解码算法：低4位 = 高4位^0x3，高4位 = 高4位^0x8^低4位
 * 事件状态机推算当前位置（计时基于行首毫秒级单调时钟，见 HEAD_COUNTER）：
 *   播放开始/恢复 -> 记录毫秒时钟基准；暂停/停止 -> 累计已播时长；
 *   setPlayingPosition -> 应用内拖动进度，重置基准。
 */

const DECODE_TABLE = new Uint8Array(256);
for (let i = 0; i < 256; i++) {
  const lo = i & 0xf;
  const hi = (i >> 4) & 0xf;
  DECODE_TABLE[i] = (hi ^ 0x3) | ((hi ^ 0x8 ^ lo) << 4);
}

/** 首次读取的尾部窗口；事件很密集，512KB 足够覆盖最近几首歌 */
const INITIAL_TAIL_BYTES = 512 * 1024;

interface PlaybackState {
  songId: string;
  playing: boolean;
  /** 累计已播放（ms），不含当前进行中的区间 */
  baseMs: number;
  /** playing 时的基准时刻（elog 毫秒时钟空间，非 epoch） */
  sinceCounter: number;
}

function elogPath(): string {
  return path.join(process.env.LOCALAPPDATA || '', 'NetEase', 'CloudMusic', 'cloudmusic.elog');
}

function decode(buf: Buffer): string {
  const out = Buffer.allocUnsafe(buf.length);
  for (let i = 0; i < buf.length; i++) out[i] = DECODE_TABLE[buf[i]];
  return out.toString('utf8');
}

/**
 * 行首形如 [pid:tid:MMDD/HHMMSS:98637531:INFO:...] [2026-07-13 17:27:56] 【playing】,...
 * 第 4 段是毫秒级单调时钟；括号内墙钟只有秒级精度，直接用它做基准每首歌会随机
 * 提前 0~1s。故状态机计时全在毫秒时钟（counter）空间进行，仅在 poll 输出时经
 * counterDelta（epoch - counter 的估计）换算到当前时刻。
 */
const HEAD_COUNTER = /^\[\d+:\d+:\d{4}\/\d{6}:(\d+):/;
const WALL_TIME = /\[(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2}):(\d{2})]/;

class NeteaseElogProvider implements PositionProvider {
  name = 'netease-elog';

  private offset = -1;
  private carry = '';
  private state: PlaybackState | null = null;
  private available: boolean | null = null;
  /**
   * epoch - counter 的估计值。每行墙钟秒满足 wallSec <= 真实时刻 < wallSec+1s，
   * 故 o = wallSecMs - counter ∈ (delta-1s, delta]，取历史最大值即从下方逼近 delta。
   * counter 时代切换（网易云重启）时 o 会跌出该区间，据此重置。
   */
  private counterDelta = Number.NEGATIVE_INFINITY;

  /** 从行中提取毫秒时钟并顺带校准 counterDelta；无法提取返回 0 */
  private lineCounter(line: string): number {
    const h = HEAD_COUNTER.exec(line);
    if (!h) return 0;
    const counter = parseInt(h[1]);
    const w = WALL_TIME.exec(line);
    if (w) {
      const wallSecMs = new Date(+w[1], +w[2] - 1, +w[3], +w[4], +w[5], +w[6]).getTime();
      const o = wallSecMs - counter;
      if (o > this.counterDelta || o < this.counterDelta - 1100) {
        this.counterDelta = o;
      }
    }
    return counter;
  }

  matches(sourceAppId: string): boolean {
    return sourceAppId.toLowerCase().includes('cloudmusic');
  }

  async poll(): Promise<PositionInfo | null> {
    try {
      const p = elogPath();
      if (this.available === null) this.available = fs.existsSync(p);
      if (!this.available) return null;

      const size = fs.statSync(p).size;
      if (this.offset < 0 || size < this.offset) {
        // 首次读取或日志被轮转：从尾部窗口重建状态
        this.offset = Math.max(0, size - INITIAL_TAIL_BYTES);
        this.carry = '';
        this.state = null;
      }
      if (size > this.offset) {
        const fd = fs.openSync(p, 'r');
        try {
          const buf = Buffer.allocUnsafe(size - this.offset);
          fs.readSync(fd, buf, 0, buf.length, this.offset);
          this.offset = size;
          const text = this.carry + decode(buf);
          const lines = text.split('\n');
          this.carry = lines.pop() ?? '';
          for (const line of lines) this.consume(line);
        } finally {
          fs.closeSync(fd);
        }
      }

      const s = this.state;
      if (!s) return null;
      // 当前时刻换算到 counter 空间再外推；delta 必然已由日志行校准过
      const nowCounter = Number.isFinite(this.counterDelta) ? Date.now() - this.counterDelta : s.sinceCounter;
      const positionMs = s.playing ? s.baseMs + Math.max(0, nowCounter - s.sinceCounter) : s.baseMs;
      return {
        positionMs: Math.max(0, Math.round(positionMs)),
        // elog 中的时长字段不可靠（曾误抓无关 JSON 的 "time"），一律 0，
        // 由上层按 songId 从网易云 API 取权威时长
        durationMs: 0,
        playing: s.playing,
        songId: s.songId,
      };
    } catch (e) {
      console.error('[NeteaseElog] poll failed:', e);
      return null;
    }
  }

  private ensureState(songId: string, cnt: number): PlaybackState {
    if (!this.state || (songId && this.state.songId !== songId)) {
      this.state = {
        songId,
        playing: false,
        baseMs: 0,
        sinceCounter: cnt,
      };
    }
    return this.state;
  }

  private markPlaying(s: PlaybackState, cnt: number) {
    if (!s.playing) {
      s.playing = true;
      s.sinceCounter = cnt;
    }
  }

  private markStopped(s: PlaybackState, cnt: number) {
    if (s.playing) {
      s.baseMs += Math.max(0, cnt - s.sinceCounter);
      s.playing = false;
    }
  }

  private consume(line: string) {
    // 每行都提取（顺带持续校准 counterDelta）
    const cnt = this.lineCounter(line);
    if (!cnt || !line.includes('playing')) return;

    // 播放状态: "native播放state",<0|1>,"<songId>_XXX"
    let m = /"native播放state",(\d+),"(\d+)_/.exec(line);
    if (m) {
      const s = this.ensureState(m[2], cnt);
      if (m[1] === '1') this.markPlaying(s, cnt);
      else this.markStopped(s, cnt);
      return;
    }
    // 开始播放命令: "nativePlay","playCommand",,,"songId"（含真实 songId，非"重置"）
    m = /"nativePlay","playCommand",[^"]*"(\d{4,})"/.exec(line);
    if (m) {
      const s = this.ensureState(m[1], cnt);
      // playCommand = 从头播放该曲目。songId 相同（单曲循环重播）时
      // ensureState 不会重建状态，须显式归零，否则进度卡在上一遍末尾
      s.baseMs = 0;
      s.playing = false;
      this.markPlaying(s, cnt);
      return;
    }
    // 应用内拖动进度: "setPlayingPosition",<value>
    m = /"setPlayingPosition",\s*([\d.]+)/.exec(line);
    if (m && this.state) {
      const v = parseFloat(m[1]);
      // 单位兼容：大于 10000 视为 ms，否则视为秒
      this.state.baseMs = v > 10000 ? v : v * 1000;
      this.state.sinceCounter = cnt;
      return;
    }
    // 停止/播完
    if (line.includes('"stop playId="') || line.includes('"onPlayEnd handle reason"')) {
      if (this.state) this.markStopped(this.state, cnt);
    }
  }
}

export const neteaseElogProvider = new NeteaseElogProvider();
