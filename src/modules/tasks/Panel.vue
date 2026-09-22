<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from '@/core/i18n';
import { tasksState, addTask, toggleTask, deleteTask, formatTaskTime, toggleShowCompleted } from './store';

const { t } = useI18n();
const snap = tasksState;

const pendingTaskCount = computed(() => snap.tasks.filter((t) => !t.done).length);
const completedCount = computed(() => snap.tasks.filter((t) => t.done).length);
const sortedTasks = computed(() => {
  const active = snap.tasks.filter((t) => !t.done);
  const completed = snap.tasks.filter((t) => t.done);
  return snap.showCompleted ? [...active, ...completed] : active;
});
</script>

<template>
  <div class="tasks-header">
    <i class="fa-solid fa-list-check tasks-icon"></i>
    <span class="tasks-title">{{ t('tasksHeader') }}</span>
    <span v-if="snap.tasks.length" class="tasks-count">{{ pendingTaskCount }}/{{ snap.tasks.length }}</span>
    <button
      v-if="completedCount"
      class="tasks-history-toggle"
      :title="snap.showCompleted ? t('tasksHideCompleted') : t('tasksShowCompleted')"
      @click.stop="toggleShowCompleted()"
    >
      <i :class="snap.showCompleted ? 'fa-solid fa-eye' : 'fa-solid fa-eye-slash'"></i>
      <span v-if="!snap.showCompleted" class="history-badge">{{ completedCount }}</span>
    </button>
  </div>
  <div class="tasks-add">
    <input
      v-model="tasksState.newTaskText"
      type="text"
      class="tasks-input"
      :placeholder="t('tasksNewPlaceholder')"
      @keydown.enter.stop="addTask()"
      @click.stop
    />
    <button class="tasks-add-btn" :disabled="!tasksState.newTaskText.trim()" @click.stop="addTask()">
      <i class="fa-solid fa-plus"></i>
    </button>
  </div>
  <div class="tasks-list">
    <div v-if="!snap.tasks.length" class="tasks-empty">
      <i class="fa-solid fa-clipboard"></i>
      <span>{{ t('tasksEmpty') }}</span>
    </div>
    <div v-for="task in sortedTasks" :key="task.id" class="task-row" :class="{ done: task.done }">
      <label class="task-check" @click.stop="toggleTask(task.id)">
        <i :class="task.done ? 'fa-solid fa-circle-check' : 'fa-regular fa-circle'"></i>
      </label>
      <div class="task-body">
        <div class="task-text">{{ task.text }}</div>
        <div v-if="task.due_time || task.source" class="task-meta">
          <span v-if="task.due_time" class="task-due">
            <i class="fa-solid fa-clock"></i> {{ formatTaskTime(task.due_time) }}
          </span>
        </div>
      </div>
      <button
        class="task-done-btn"
        :title="task.done ? t('taskRestore') : t('taskComplete')"
        @click.stop="toggleTask(task.id)"
      >
        <i :class="task.done ? 'fa-solid fa-rotate-left' : 'fa-solid fa-check'"></i>
      </button>
      <button class="task-del" @click.stop="deleteTask(task.id)">
        <i class="fa-solid fa-trash"></i>
      </button>
    </div>
  </div>
</template>
