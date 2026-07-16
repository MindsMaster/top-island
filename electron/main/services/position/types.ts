export interface PositionInfo {
  positionMs: number;
  /** 0 表示该源不知道时长（可由上层按 songId 从平台 API 补齐） */
  durationMs: number;
  playing: boolean;
  /** 平台侧歌曲 ID（如网易云 songId）；有则可按 ID 精确取歌词/时长 */
  songId?: string;
}

export interface PositionProvider {
  name: string;
  /** 是否负责该 SMTC 会话（按 SourceAppUserModelId 判断） */
  matches(sourceAppId: string): boolean;
  poll(): Promise<PositionInfo | null>;
}
