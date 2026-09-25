<script setup lang="ts">
import { computed } from 'vue';
import { settings } from '@/core/settings';
import { useI18n } from '@/core/i18n';
import {
  closeTopPopup,
  formatTime,
  imageFailed,
  notifyState as snap,
  openPopups,
  setPopupHover,
} from './store';

const { t } = useI18n();

const entry = computed(() => snap.popups[0]);
const privacy = computed(() => settings.notifications.privacy);
</script>

<template>
  <div
    v-if="entry"
    class="notify-strip"
    @mouseenter="setPopupHover(true)"
    @mouseleave="setPopupHover(false)"
    @click.stop="openPopups()"
  >
    <div :key="entry.key" class="notify-strip-body">
      <div class="notify-avatar" :class="{ 'notify-blur': privacy.enabled && privacy.blurAvatar }">
        <img
          v-if="snap.images[entry.key]"
          :src="snap.images[entry.key]"
          alt=""
          draggable="false"
          @error="imageFailed(entry, $event)"
        />
        <span v-else class="notify-avatar-fallback">{{ (entry.app || '?').slice(0, 1) }}</span>
      </div>
      <div class="notify-body">
        <div class="notify-meta">
          <span class="notify-app">{{ entry.app }}</span>
          <span class="notify-time">{{ formatTime(entry.arrival) }}</span>
        </div>
        <div
          v-if="entry.title"
          class="notify-title"
          :class="{ 'notify-blur': privacy.enabled && privacy.blurName }"
        >
          {{ entry.title }}
        </div>
        <div v-if="privacy.enabled && privacy.replaceBody" class="notify-text notify-text-private">
          {{ privacy.bodyText || t('notifyPrivateBody') }}
        </div>
        <div v-else-if="entry.body" class="notify-text">{{ entry.body }}</div>
      </div>
    </div>
    <button class="notify-close" @click.stop="closeTopPopup()">
      <i class="fa-solid fa-xmark"></i>
    </button>
  </div>
</template>
