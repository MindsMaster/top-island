export interface AppEvents {
  /** 响铃起止 */
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
