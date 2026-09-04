<script setup lang="ts">
import { computed, ref } from 'vue';
import { useI18n } from '../i18n';
import { fmtLocalDate, todayLocalStr } from '../composables/useClock';
import { tasksState, addTask, toggleTask, deleteTask, formatTaskTime } from '../store/tasks';

const { t, lang } = useI18n();
const snap = tasksState;

const selectedDate = ref(todayLocalStr());
const showMonthPicker = ref(false);
const monthOffset = ref(0);
const newTaskText = ref('');
const newTaskHour = ref('09');
const newTaskMinute = ref('00');
const newTaskAllDay = ref(false);

const weekDayLabels = computed(() =>
  lang.value === 'en-US'
    ? ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']
    : ['一', '二', '三', '四', '五', '六', '日']
);

function mondayOf(dateStr: string): Date {
  const d = new Date(dateStr + 'T00:00:00');
  const day = d.getDay() || 7;
  const monday = new Date(d);
  monday.setDate(d.getDate() - day + 1);
  return monday;
}

const weekRange = computed(() => {
  const monday = mondayOf(selectedDate.value);
  const sunday = new Date(monday);
  sunday.setDate(monday.getDate() + 6);
  if (lang.value === 'en-US') {
    const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
    return `${months[monday.getMonth()]} ${monday.getDate()} - ${months[sunday.getMonth()]} ${sunday.getDate()}`;
  }
  return `${monday.getMonth() + 1}月${monday.getDate()}日 - ${sunday.getMonth() + 1}月${sunday.getDate()}日`;
});

const weekDates = computed(() => {
  const monday = mondayOf(selectedDate.value);
  const today = todayLocalStr();
  const days = [];
  for (let i = 0; i < 7; i++) {
    const dt = new Date(monday);
    dt.setDate(monday.getDate() + i);
    const ds = fmtLocalDate(dt);
    days.push({
      date: ds,
      day: dt.getDate(),
      isToday: ds === today,
      hasTask: snap.tasks.some((tk) => !tk.done && tk.due_time && tk.due_time.slice(0, 10) === ds),
    });
  }
  return days;
});

const selectedDateLabel = computed(() => {
  const d = new Date(selectedDate.value + 'T00:00:00');
  const wdIdx = d.getDay() === 0 ? 6 : d.getDay() - 1;
  if (lang.value === 'en-US') {
    const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
    return `${months[d.getMonth()]} ${d.getDate()} ${weekDayLabels.value[wdIdx]}`;
  }
  return `${d.getMonth() + 1}月${d.getDate()}日 周${weekDayLabels.value[wdIdx]}`;
});

const dayTasks = computed(() =>
  snap.tasks
    .filter((tk) => tk.due_time && tk.due_time.slice(0, 10) === selectedDate.value)
    .sort((a, b) => {
      if (a.all_day && !b.all_day) return -1;
      if (!a.all_day && b.all_day) return 1;
      if (a.all_day && b.all_day) return 0;
      return (a.due_time || '').localeCompare(b.due_time || '');
    })
);

const monthPickerLabel = computed(() => {
  const now = new Date();
  const m = new Date(now.getFullYear(), now.getMonth() + monthOffset.value, 1);
  if (lang.value === 'en-US') {
    const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
    return `${months[m.getMonth()]} ${m.getFullYear()}`;
  }
  return `${m.getFullYear()}年${m.getMonth() + 1}月`;
});

const monthCalendarDays = computed(() => {
  const now = new Date();
  const m = new Date(now.getFullYear(), now.getMonth() + monthOffset.value, 1);
  const year = m.getFullYear();
  const month = m.getMonth();
  const lastDay = new Date(year, month + 1, 0);
  const startDow = new Date(year, month, 1).getDay() || 7;
  const today = todayLocalStr();
  const days = [];
  for (let i = 1; i < startDow; i++) {
    const pd = new Date(year, month, 1 - (startDow - i));
    const ds = fmtLocalDate(pd);
    days.push({
      date: ds,
      day: pd.getDate(),
      otherMonth: true,
      isToday: ds === today,
      hasTask: false,
    });
  }
  for (let d = 1; d <= lastDay.getDate(); d++) {
    const ds = fmtLocalDate(new Date(year, month, d));
    days.push({
      date: ds,
      day: d,
      otherMonth: false,
      isToday: ds === today,
      hasTask: snap.tasks.some((tk) => !tk.done && tk.due_time && tk.due_time.slice(0, 10) === ds),
    });
  }
  return days;
});

function prevWeek() {
  const d = new Date(selectedDate.value + 'T00:00:00');
  d.setDate(d.getDate() - 7);
  selectedDate.value = fmtLocalDate(d);
}

function nextWeek() {
  const d = new Date(selectedDate.value + 'T00:00:00');
  d.setDate(d.getDate() + 7);
  selectedDate.value = fmtLocalDate(d);
}

function jumpToDateFromMonth(dateStr: string) {
  selectedDate.value = dateStr;
  showMonthPicker.value = false;
  monthOffset.value = 0;
}

function addCalendarTask() {
  const text = newTaskText.value.trim();
  if (!text) return;
  const dueTime = newTaskAllDay.value
    ? selectedDate.value + 'T00:00:00'
    : selectedDate.value + 'T' + newTaskHour.value + ':' + newTaskMinute.value + ':00';
  addTask(text, dueTime, null, newTaskAllDay.value, 'user');
  newTaskText.value = '';
  newTaskHour.value = '09';
  newTaskMinute.value = '00';
  newTaskAllDay.value = false;
}
</script>

<template>
  <div class="cal-week-nav">
    <button class="cal-nav-btn" @click.stop="prevWeek">
      <i class="fa-solid fa-chevron-left"></i>
    </button>
    <span class="cal-week-range">{{ weekRange }}</span>
    <button class="cal-nav-btn" @click.stop="nextWeek">
      <i class="fa-solid fa-chevron-right"></i>
    </button>
    <button class="cal-month-btn" @click.stop="showMonthPicker = !showMonthPicker">
      <i class="fa-solid fa-calendar-days"></i>
    </button>
  </div>
  <div class="cal-week-header">
    <span v-for="d in weekDayLabels" :key="d" class="cal-day-label">{{ d }}</span>
  </div>
  <div class="cal-week-days">
    <div
      v-for="d in weekDates"
      :key="d.date"
      class="cal-day-cell"
      :class="{ today: d.isToday, selected: d.date === selectedDate }"
      @click.stop="selectedDate = d.date"
    >
      <span class="cal-day-num">{{ d.day }}</span>
      <span v-if="d.hasTask" class="cal-day-dots">
        <span v-if="d.hasTask" class="cal-day-dot"></span>
      </span>
    </div>
  </div>
  <div class="cal-day-header">
    <i class="fa-solid fa-calendar-check"></i>
    <span>{{ selectedDateLabel }}</span>
  </div>
  <div class="cal-tasks-list">
    <div v-if="!dayTasks.length" class="cal-tasks-empty">
      <i class="fa-regular fa-calendar"></i>
      <span>{{ t('calendarEmpty') }}</span>
    </div>
    <div v-for="task in dayTasks" :key="task.id" class="cal-task-row" :class="{ done: task.done }">
      <label class="cal-task-check" @click.stop="toggleTask(task.id)">
        <i :class="task.done ? 'fa-solid fa-circle-check' : 'fa-regular fa-circle'"></i>
      </label>
      <div class="cal-task-body">
        <div class="cal-task-text">{{ task.text }}</div>
        <div v-if="!task.all_day" class="cal-task-time">
          <i class="fa-regular fa-clock"></i> {{ formatTaskTime(task.due_time) }}
        </div>
        <div v-else class="cal-task-time">
          <i class="fa-regular fa-calendar"></i> {{ t('calendarAllDay') }}
        </div>
      </div>
      <button class="cal-task-done-btn" @click.stop="toggleTask(task.id)">
        <i :class="task.done ? 'fa-solid fa-rotate-left' : 'fa-solid fa-check'"></i>
      </button>
      <button class="cal-task-del" @click.stop="deleteTask(task.id)">
        <i class="fa-solid fa-trash"></i>
      </button>
    </div>
  </div>
  <div class="cal-add">
    <input
      v-model="newTaskText"
      type="text"
      class="cal-add-input"
      :placeholder="t('calendarNewPlaceholder')"
      @keydown.enter.stop="addCalendarTask"
      @click.stop
    />
    <select v-if="!newTaskAllDay" v-model="newTaskHour" class="cal-time-sel" @click.stop>
      <option v-for="h in 24" :key="h" :value="String(h - 1).padStart(2, '0')">
        {{ String(h - 1).padStart(2, '0') }}
      </option>
    </select>
    <span v-if="!newTaskAllDay" class="cal-time-sep">:</span>
    <select v-if="!newTaskAllDay" v-model="newTaskMinute" class="cal-time-sel" @click.stop>
      <option v-for="m in 12" :key="m" :value="String((m - 1) * 5).padStart(2, '0')">
        {{ String((m - 1) * 5).padStart(2, '0') }}
      </option>
    </select>
    <button
      class="cal-add-all-day"
      :class="{ active: newTaskAllDay }"
      :title="t('calendarAllDayTitle')"
      @click.stop="newTaskAllDay = !newTaskAllDay"
    >
      <i class="fa-regular fa-calendar"></i>
    </button>
    <button class="cal-add-btn" :disabled="!newTaskText.trim()" @click.stop="addCalendarTask">
      <i class="fa-solid fa-plus"></i>
    </button>
  </div>
  <div v-if="showMonthPicker" class="cal-month-picker" @click.stop>
    <div class="cal-month-picker-nav">
      <button class="cal-nav-btn" @click.stop="monthOffset--">
        <i class="fa-solid fa-chevron-left"></i>
      </button>
      <span>{{ monthPickerLabel }}</span>
      <button class="cal-nav-btn" @click.stop="monthOffset++">
        <i class="fa-solid fa-chevron-right"></i>
      </button>
      <button class="cal-month-close" @click.stop="showMonthPicker = false">
        <i class="fa-solid fa-xmark"></i>
      </button>
    </div>
    <div class="cal-month-grid-header">
      <span v-for="d in weekDayLabels" :key="d">{{ d }}</span>
    </div>
    <div class="cal-month-grid">
      <div
        v-for="cell in monthCalendarDays"
        :key="cell.date"
        class="cal-month-cell"
        :class="{ today: cell.isToday, selected: cell.date === selectedDate, other: cell.otherMonth }"
        @click.stop="jumpToDateFromMonth(cell.date)"
      >
        <span class="cal-month-num">{{ cell.day }}</span>
        <span v-if="cell.hasTask" class="cal-month-dots">
          <span v-if="cell.hasTask" class="cal-month-dot"></span>
        </span>
      </div>
    </div>
  </div>
</template>
