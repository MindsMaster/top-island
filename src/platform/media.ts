import { convertFileSrc } from '@tauri-apps/api/core';

/** 与 src-tauri/src/ipc/media.rs 路由一致 */
export type MediaRoute = 'artwork';

export function mediaUrl(route: MediaRoute, arg: string): string {
  return convertFileSrc(`${route}/${arg}`, 'island');
}
