import { computed, ref } from 'vue';
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

const tasks = ref<TaskItem[]>([]);
const newTaskText = ref('');
const showCompleted = ref(false);
/** 到期提醒中的任务（岛面上持久显示，直到完成或超过结束时间） */
const activeReminderTask = ref<TaskItem | null>(null);

let reminderTimer: number | null = null;

async function initTasks() {
  tasks.value = (await api.storeGet<TaskItem[]>('tasks')) || [];
}

function save() {
  api.storeSet('tasks', JSON.parse(JSON.stringify(tasks.value)));
}

function addTask(
  text?: string,
  dueTime?: string | null,
  endTime?: string | null,
  allDay?: boolean,
  source?: 'user' | 'ai'
) {
  if (typeof text !== 'string') text = newTaskText.value;
  const t = (text || '').trim();
  if (!t) return;
  const dTime = dueTime || null;
  const eTime = endTime || null;
  const endVal = eTime || (dTime ? new Date(new Date(dTime).getTime() + 3600000).toISOString() : null);
  tasks.value.push({
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
  newTaskText.value = '';
  save();
}

function toggleTask(id: string) {
  const task = tasks.value.find((t) => t.id === id);
  if (!task) return;
  task.done = !task.done;
  save();
  if (task.done && activeReminderTask.value && activeReminderTask.value.id === id) {
    activeReminderTask.value = null;
  }
}

function deleteTask(id: string) {
  if (activeReminderTask.value && activeReminderTask.value.id === id) {
    activeReminderTask.value = null;
  }
  tasks.value = tasks.value.filter((t) => t.id !== id);
  save();
}

function formatTaskTime(isoStr: string | null): string {
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
  for (const task of tasks.value) {
    if (task.done || task.notified || !task.due_time) continue;
    const due = new Date(task.due_time).getTime();
    if (isNaN(due)) continue;
    const endTime = task.end_time ? new Date(task.end_time).getTime() : due + 3600000;
    if (now < due) continue;
    if (now >= endTime) {
      task.notified = true;
      if (activeReminderTask.value && activeReminderTask.value.id === task.id) {
        activeReminderTask.value = null;
      }
      continue;
    }
    task.notified = true;
    triggered = true;
    activeReminderTask.value = task;
  }
  if (triggered) save();
}

function startReminderTimer() {
  if (reminderTimer !== null) return;
  checkReminders();
  reminderTimer = window.setInterval(checkReminders, 30000);
}

function completeReminderTask() {
  if (!activeReminderTask.value) return;
  const task = tasks.value.find((t) => t.id === activeReminderTask.value!.id);
  if (task) {
    task.done = true;
    save();
  }
  activeReminderTask.value = null;
}

function dismissReminder() {
  activeReminderTask.value = null;
}

const pendingTaskCount = computed(() => tasks.value.filter((t) => !t.done).length);
const completedCount = computed(() => tasks.value.filter((t) => t.done).length);
const sortedTasks = computed(() => {
  const active = tasks.value.filter((t) => !t.done);
  const completed = tasks.value.filter((t) => t.done);
  return showCompleted.value ? [...active, ...completed] : active;
});

export function useTasks() {
  return {
    tasks,
    newTaskText,
    showCompleted,
    activeReminderTask,
    pendingTaskCount,
    completedCount,
    sortedTasks,
    initTasks,
    addTask,
    toggleTask,
    deleteTask,
    formatTaskTime,
    startReminderTimer,
    completeReminderTask,
    dismissReminder,
  };
}
