import type { IslandModule } from '../types';
import Panel from './Panel.vue';
import Stack from './Stack.vue';
import { initNotifications, notifyState } from './store';

export const notificationsModule: IslandModule = {
  id: 'notifications',
  setup: initNotifications,
  panels: [{ id: 'messages', icon: 'fa-comment-dots', titleKey: 'navMessages', component: Panel }],
  overlay: Stack,
  keepInteractive: () => notifyState.hoveredPopup !== null,
};
