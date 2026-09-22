/** Tauri IPC 的唯一出入口。各域模块只从这里取 call/on，不直接碰 @tauri-apps/api。 */
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export function call<T = void>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args);
}

/** 订阅后端事件。监听注册本身是异步的，调用方不关心何时就绪。 */
export function on<T>(event: string, cb: (payload: T) => void): void {
  void listen<T>(event, (e) => cb(e.payload));
}
