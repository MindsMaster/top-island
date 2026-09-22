import { shellView } from '@/shell/view';
import type { IslandModule } from '../types';
import Capsule from './Capsule.vue';
import Mini from './Mini.vue';
import Panel from './Panel.vue';
import { alarmState, initAlarm, keepInteractive } from './store';
import { miniHovered } from './mini';

export const alarmModule: IslandModule = {
  id: 'alarm',
  setup: initAlarm,
  panels: [{ id: 'alarm', icon: 'fa-bell', titleKey: 'navAlarm', component: Panel }],
  capsule: {
    priority: 10,
    active: () => alarmState.countdown.running && shellView.mode !== 'large',
    component: Capsule,
  },
  overlay: Mini,
  expandTarget: () => (alarmState.countdown.running ? 'alarm' : null),
  keepInteractive: () => miniHovered.value,
  holdContent: () => keepInteractive.value,
};
