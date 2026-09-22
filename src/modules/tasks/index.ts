import { shellView } from '@/shell/view';
import type { IslandModule } from '../types';
import CalendarPanel from './CalendarPanel.vue';
import Capsule from './Capsule.vue';
import Panel from './Panel.vue';
import { initTasks, startReminderTimer, tasksState } from './store';

/** 提醒中的任务在收起态占满胶囊，优先级高于歌词 */
const hasReminder = () => tasksState.activeReminderTask !== null;

export const tasksModule: IslandModule = {
  id: 'tasks',
  setup: async () => {
    await initTasks();
    startReminderTimer();
  },
  panels: [
    { id: 'tasks', icon: 'fa-check', titleKey: 'navTasks', component: Panel },
    { id: 'calendar', icon: 'fa-calendar-days', titleKey: 'navCalendar', component: CalendarPanel },
  ],
  capsule: {
    priority: 30,
    active: () => hasReminder() && shellView.mode !== 'large',
    component: Capsule,
  },
  holdContent: hasReminder,
};
