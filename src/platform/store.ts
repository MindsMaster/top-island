import { call } from './invoke';

export const storeApi = {
  get: <T>(key: string) => call<T | null>('store_get', { key }),
  set: (key: string, value: unknown) => call('store_set', { key, value }),
  /** 清空数据并重启 不可撤销 */
  clear: () => call('store_clear'),
};
