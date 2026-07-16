<script setup lang="ts">
import { useI18n } from '../i18n';
import { useTasks } from '../composables/useTasks';

const { t } = useI18n();
const {
  tasks,
  newTaskText,
  showCompleted,
  pendingTaskCount,
  completedCount,
  sortedTasks,
  addTask,
  toggleTask,
  deleteTask,
  formatTaskTime,
} = useTasks();
</script>

<template>
  <div class="tasks-header">
    <i class="fa-solid fa-list-check tasks-icon"></i>
    <span class="tasks-title">{{ t('tasksHeader') }}</span>
    <span v-if="tasks.length" class="tasks-count">{{ pendingTaskCount }}/{{ tasks.length }}</span>
    <button
      v-if="completedCount"
      class="tasks-history-toggle"
      :title="showCompleted ? t('tasksHideCompleted') : t('tasksShowCompleted')"
      @click.stop="showCompleted = !showCompleted"
    >
      <i :class="showCompleted ? 'fa-solid fa-eye' : 'fa-solid fa-eye-slash'"></i>
      <span v-if="!showCompleted" class="history-badge">{{ completedCount }}</span>
    </button>
  </div>
  <div class="tasks-add">
    <input
      v-model="newTaskText"
      type="text"
      class="tasks-input"
      :placeholder="t('tasksNewPlaceholder')"
      @keydown.enter.stop="addTask()"
      @click.stop
    />
    <button class="tasks-add-btn" :disabled="!newTaskText.trim()" @click.stop="addTask()">
      <i class="fa-solid fa-plus"></i>
    </button>
  </div>
  <div class="tasks-list">
    <div v-if="!tasks.length" class="tasks-empty">
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
