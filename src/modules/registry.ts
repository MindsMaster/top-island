import type { IslandModule } from './types';
import { alarmModule } from './alarm';
import { clipboardModule } from './clipboard';
import { musicModule } from './music';
import { notificationsModule } from './notifications';
import { tasksModule } from './tasks';
import { weatherModule } from './weather';

/** 顺序决定面板页序与优先级 */
export const modules: IslandModule[] = [
  weatherModule,
  musicModule,
  tasksModule,
  alarmModule,
  clipboardModule,
  notificationsModule,
];

export const panels = modules.flatMap((m) => m.panels ?? []);

export function panelIndex(id: string): number {
  return panels.findIndex((p) => p.id === id);
}
