<script setup lang="ts">
import { useI18n } from '@/core/i18n';
import { completeReminderTask, tasksState as snap } from './store';

const { t } = useI18n();
</script>

<template>
  <div class="reminder-content">
    <div class="reminder-top">
      <i class="fa-solid fa-clock reminder-icon"></i>
      <span class="reminder-text">{{ snap.activeReminderTask!.text }}</span>
      <button class="reminder-done-btn" @click.stop="completeReminderTask()">
        <i class="fa-solid fa-check"></i>
      </button>
    </div>
    <div class="reminder-hint">{{ t('reminderHint') }}</div>
  </div>
</template>
