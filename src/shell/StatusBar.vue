<script setup lang="ts">
import { settingsApi } from '@/platform/settings';
import { windowApi } from '@/platform/window';
import { currentDate, currentTime } from '@/core/clock';
import { useI18n } from '@/core/i18n';
import { settings, toggleTheme } from '@/core/settings';
import { themeMeta } from '@/core/theme';
import { modules } from '@/modules/registry';

const { t } = useI18n();

const chips = modules.filter((m) => m.chip);
</script>

<template>
  <div class="quick-content">
    <component :is="m.chip" v-for="m in chips" :key="m.id" />
    <span class="quick-time">{{ currentTime }}</span>
    <span class="quick-date">{{ currentDate }}</span>
    <div class="quick-right">
      <button class="quick-settings-btn" :title="t('openSettings')" @click.stop="settingsApi.open()">
        <i class="fa-solid fa-gear"></i>
      </button>
      <button class="quick-theme-btn" :title="t('themeCycle')" @click.stop="toggleTheme">
        <i :class="'fa-solid ' + themeMeta(settings.theme).icon"></i>
      </button>
      <button class="quick-close-btn" :title="t('closeIsland')" @click.stop="windowApi.quit()">
        <i class="fa-solid fa-xmark"></i>
      </button>
    </div>
  </div>
</template>
