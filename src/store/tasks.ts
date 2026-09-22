import { reactive } from 'vue';
import { api } from '../api';

export interface TaskItem {
  id: string;
  text: string;
  done: boolean;
  due_time: string | null;
  end_time: string | null;
  all_day: boolean;
  source: 'user' | 'ai';
  notified: boolean;
  created_at: number;
}

/** 任务域共享状态。组件模板直读 tasksState 字段，
 *  写一律走下面的 action；newTaskText 是输入框 v-model 的唯一例外，可直接写 proxy。
 *  派生数据（pending/completed/sorted）不放这里，组件里 computed(snap.xxx) 自取 */
export const tasksState = reactive({
  tasks: [] as TaskItem[],
  newTaskText: '',
  showCompleted: false,
  /** 到期提醒中的任务（岛面上持久显示，直到完成或超过结束时间） */
  activeReminderTask: null as TaskItem | null,
});

/** 提醒轮询句柄：内部簿记，不进 proxy */
let reminderTimer: number | null = null;

export async function initTasks() {
  tasksState.tasks = (await api.storeGet<TaskItem[]>('tasks')) || [];
}

/** 持久化：整个任务列表落盘。reactive 对 JSON.stringify 透明，无需先取 raw */
function save() {
  api.storeSet('tasks', JSON.parse(JSON.stringify(tasksState.tasks)));
}

export function addTask(
  text?: string,
  dueTime?: string | null,
  endTime?: string | null,
  allDay?: boolean,
  source?: 'user' | 'ai'
) {
  if (typeof text !== 'string') text = tasksState.newTaskText;
  const t = (text || '').trim();
  if (!t) return;
  const dTime = dueTime || null;
  const eTime = endTime || null;
  const endVal = eTime || (dTime ? new Date(new Date(dTime).getTime() + 3600000).toISOString() : null);
  tasksState.tasks.push({
    id: Date.now().toString(36) + Math.random().toString(36).slice(2, 7),
    text: t,
    done: false,
    due_time: dTime,
    end_time: endVal,
    all_day: allDay || false,
    source: source || 'user',
    notified: false,
    created_at: Date.now(),
  });
  tasksState.newTaskText = '';
  save();
}

export function toggleTask(id: string) {
  const task = tasksState.tasks.find((t) => t.id === id);
  if (!task) return;
  task.done = !task.done;
  save();
  if (task.done && tasksState.activeReminderTask && tasksState.activeReminderTask.id === id) {
    tasksState.activeReminderTask = null;
  }
}

export function deleteTask(id: string) {
  if (tasksState.activeReminderTask && tasksState.activeReminderTask.id === id) {
    tasksState.activeReminderTask = null;
  }
  tasksState.tasks = tasksState.tasks.filter((t) => t.id !== id);
  save();
}

/** 面板"显示/隐藏已完成"开关（收敛成 action，写路径统一） */
export function toggleShowCompleted() {
  tasksState.showCompleted = !tasksState.showCompleted;
}

export function formatTaskTime(isoStr: string | null): string {
  if (!isoStr) return '';
  const d = new Date(isoStr);
  if (isNaN(d.getTime())) {
    const m = /(\d{1,2}):(\d{2})/.exec(isoStr);
    return m ? m[1] + ':' + m[2] : isoStr;
  }
  return (
    d.getMonth() +
    1 +
    '/' +
    d.getDate() +
    ' ' +
    String(d.getHours()).padStart(2, '0') +
    ':' +
    String(d.getMinutes()).padStart(2, '0')
  );
}

function checkReminders() {
  const now = Date.now();
  let triggered = false;
  for (const task of tasksState.tasks) {
    if (task.done || task.notified || !task.due_time) continue;
    const due = new Date(task.due_time).getTime();
    if (isNaN(due)) continue;
    const endTime = task.end_time ? new Date(task.end_time).getTime() : due + 3600000;
    if (now < due) continue;
    if (now >= endTime) {
      task.notified = true;
      if (tasksState.activeReminderTask && tasksState.activeReminderTask.id === task.id) {
        tasksState.activeReminderTask = null;
      }
      continue;
    }
    task.notified = true;
    triggered = true;
    tasksState.activeReminderTask = task;
  }
  // 只有真正触发了提醒才落盘（notified 标志要持久化），纯巡检不写盘
  if (triggered) save();
}

export function startReminderTimer() {
  if (reminderTimer !== null) return;
  checkReminders();
  reminderTimer = window.setInterval(checkReminders, 30000);
}

export function completeReminderTask() {
  if (!tasksState.activeReminderTask) return;
  const task = tasksState.tasks.find((t) => t.id === tasksState.activeReminderTask!.id);
  if (task) {
    task.done = true;
    save();
  }
  tasksState.activeReminderTask = null;
}
