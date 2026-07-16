import type { PositionInfo, PositionProvider } from './position/types';
import { neteaseElogProvider } from './position/netease-elog';

const PROVIDERS: PositionProvider[] = [neteaseElogProvider];

/** 为指定来源应用查询外部位置源；无 provider 命中或读取失败返回 null */
export async function pollExternalPosition(sourceAppId: string): Promise<PositionInfo | null> {
  for (const provider of PROVIDERS) {
    if (!provider.matches(sourceAppId)) continue;
    try {
      const info = await provider.poll();
      if (info) return info;
    } catch (e) {
      console.error(`[Position] provider ${provider.name} failed:`, e);
    }
  }
  return null;
}

export type { PositionInfo, PositionProvider } from './position/types';
