import { shellView } from '@/shell/view';
import type { IslandModule } from '../types';
import Capsule from './Capsule.vue';
import Layers from './Layers.vue';
import Panel from './Panel.vue';
import { initNotifications, notifyState } from './store';

const hasPopup = () => notifyState.popups.length > 0;

export const notificationsModule: IslandModule = {
  id: 'notifications',
  setup: initNotifications,
  panels: [{ id: 'messages', icon: 'fa-comment-dots', titleKey: 'navMessages', component: Panel }],
  capsule: {
    priority: 25,
    active: () => hasPopup() && shellView.mode !== 'large',
    component: Capsule,
  },
  overlay: Layers,
  holdContent: hasPopup,
};
