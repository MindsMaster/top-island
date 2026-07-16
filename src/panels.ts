import type { Component } from 'vue';
import WeatherPanel from './panels/WeatherPanel.vue';
import MusicPanel from './panels/MusicPanel.vue';
import TasksPanel from './panels/TasksPanel.vue';
import CalendarPanel from './panels/CalendarPanel.vue';
import AlarmPanel from './panels/AlarmPanel.vue';
import ClipboardPanel from './panels/ClipboardPanel.vue';
import MessagesPanel from './panels/MessagesPanel.vue';

export interface PanelDef {
  /** 同时作为面板容器 class（`<id>-panel`），供 SCSS 选择器使用 */
  id: string;
  /** FontAwesome 图标类（不含 fa-solid 前缀） */
  icon: string;
  /** 指示器 tooltip 的 i18n key */
  titleKey: string;
  component: Component;
}

export const panels: PanelDef[] = [
  { id: 'weather', icon: 'fa-cloud-sun', titleKey: 'navWeather', component: WeatherPanel },
  { id: 'music', icon: 'fa-music', titleKey: 'navMusic', component: MusicPanel },
  { id: 'tasks', icon: 'fa-check', titleKey: 'navTasks', component: TasksPanel },
  { id: 'calendar', icon: 'fa-calendar-days', titleKey: 'navCalendar', component: CalendarPanel },
  { id: 'alarm', icon: 'fa-bell', titleKey: 'navAlarm', component: AlarmPanel },
  { id: 'clipboard', icon: 'fa-clipboard', titleKey: 'navClipboard', component: ClipboardPanel },
  { id: 'messages', icon: 'fa-comment-dots', titleKey: 'navMessages', component: MessagesPanel },
];

/** 日历面板索引（任务到期提醒点击时跳转用） */
export const CALENDAR_PANEL_INDEX = panels.findIndex((p) => p.id === 'calendar');
/** 闹钟面板索引（倒计时运行时点击展开跳转用） */
export const ALARM_PANEL_INDEX = panels.findIndex((p) => p.id === 'alarm');
