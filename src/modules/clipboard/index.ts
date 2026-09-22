import type { IslandModule } from '../types';
import Panel from './Panel.vue';
import { initClipboard, readCurrent, startClipboardWatch } from './store';

export const clipboardModule: IslandModule = {
  id: 'clipboard',
  setup: async () => {
    await initClipboard();
    startClipboardWatch();
    readCurrent();
  },
  panels: [{ id: 'clipboard', icon: 'fa-clipboard', titleKey: 'navClipboard', component: Panel }],
};
