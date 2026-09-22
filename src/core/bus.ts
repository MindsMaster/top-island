/**
 * 模块间事件总线。模块之间不互相 import：发事件的一方不关心谁在听，
 * 听的一方自己决定怎么响应（例：闹钟响铃 -> 音乐模块自行暂停）。
 */
export interface AppEvents {
  /** 闹钟/倒计时开始或结束响铃 */
  'alarm:ringing': boolean;
}

type Handler<K extends keyof AppEvents> = (payload: AppEvents[K]) => void;

const handlers = new Map<keyof AppEvents, Set<Handler<never>>>();

export function on<K extends keyof AppEvents>(event: K, fn: Handler<K>): void {
  let set = handlers.get(event);
  if (!set) handlers.set(event, (set = new Set()));
  set.add(fn as Handler<never>);
}

export function emit<K extends keyof AppEvents>(event: K, payload: AppEvents[K]): void {
  for (const fn of handlers.get(event) ?? []) (fn as Handler<K>)(payload);
}
