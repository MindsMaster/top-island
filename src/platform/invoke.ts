import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export function call<T = void>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args);
}

export function on<T>(event: string, cb: (payload: T) => void): void {
  void listen<T>(event, (e) => cb(e.payload));
}
