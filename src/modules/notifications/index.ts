import { shellView } from '@/shell/view';
import type { IslandModule } from '../types';
import Capsule from './Capsule.vue';
import Panel from './Panel.vue';
import { initNotifications, notifyState } from './store';

const hasPopup = () => notifyState.popup !== null;

export const notificationsModule: IslandModule = {
  id: 'notifications',
  setup: initNotifications,
  panels: [
    { id: 'messages', icon: 'fa-comment-dots', titleKey: 'navMessages', component: Panel, badge: hasPopup },
  ],
  capsule: {
    priority: 25,
    active: () => hasPopup() && shellView.mode !== 'large',
    component: Capsule,
  },
  holdContent: hasPopup,
};
