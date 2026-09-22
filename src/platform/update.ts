import { call, on } from './invoke';
import type { UpdateCheckResult } from './types';

export const updateApi = {
  status: () => call<UpdateCheckResult>('update_status'),
  check: () => call<UpdateCheckResult>('update_check'),
  install: () => call('update_install'),
  onDownloaded: (cb: (info: { version: string }) => void) => on<{ version: string }>('update:downloaded', cb),
};
