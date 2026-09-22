<script setup lang="ts">
import { computed, ref } from 'vue';
import { settings } from '@/core/settings';
import { useI18n } from '@/core/i18n';
import { shellView } from '@/shell/view';
import { contributeHotRect } from '@/shell/useHotRegion';
import {
  activatePopup,
  closePopup,
  foldedCount,
  formatTime,
  notifyState as snap,
  setPopupHover,
  visiblePopups,
} from './store';

const { t } = useI18n();

const el = ref<HTMLElement | null>(null);
contributeHotRect(el);

const privacy = computed(() => settings.notifications.privacy);
</script>

<template>
  <TransitionGroup
    ref="el"
    name="npop"
    tag="div"
    class="notify-stack"
    :class="{ 'dock-hidden': shellView.hidden, 'dock-large': shellView.mode === 'large' }"
  >
    <div
      v-for="card in visiblePopups"
      :key="card.key"
      class="notify-card"
      @mouseenter="setPopupHover(card.key)"
      @mouseleave="setPopupHover(null)"
      @click.stop="activatePopup(card)"
    >
      <div class="notify-avatar" :class="{ 'notify-blur': privacy.enabled && privacy.blurAvatar }">
        <img v-if="snap.images[card.entry.key]" :src="snap.images[card.entry.key]" alt="" draggable="false" />
        <span v-else class="notify-avatar-fallback">{{ (card.entry.app || '?').slice(0, 1) }}</span>
      </div>
      <div class="notify-body">
        <div class="notify-meta">
          <span class="notify-app">{{ card.entry.app }}</span>
          <span class="notify-time">{{ formatTime(card.entry.arrival) }}</span>
        </div>
        <div
          v-if="card.entry.title"
          class="notify-title"
          :class="{ 'notify-blur': privacy.enabled && privacy.blurName }"
        >
          {{ card.entry.title }}
        </div>
        <div v-if="privacy.enabled && privacy.replaceBody" class="notify-text notify-text-private">
          {{ privacy.bodyText || t('notifyPrivateBody') }}
        </div>
        <div v-else-if="card.entry.body" class="notify-text">{{ card.entry.body }}</div>
      </div>
      <button class="notify-close" @click.stop="closePopup(card.key)">
        <i class="fa-solid fa-xmark"></i>
      </button>
    </div>
    <div v-if="foldedCount > 0" key="__fold" class="notify-fold">
      {{ t('notifyFolded', foldedCount) }}
    </div>
  </TransitionGroup>
</template>
