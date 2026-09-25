<script setup lang="ts">
import { settingsApi } from '@/platform/settings';
import { useI18n } from '@/core/i18n';
import { panels } from '@/modules/registry';
import type { usePanelDeck } from './usePanelDeck';

const { deck } = defineProps<{ deck: ReturnType<typeof usePanelDeck>; visible: boolean }>();

const { t } = useI18n();
</script>

<template>
  <template v-for="(p, i) in panels" :key="p.id">
    <div
      v-if="p.backdrop && deck.isMounted(i)"
      class="panel-backdrop"
      :class="{ active: visible && deck.activePanel.value === i }"
    >
      <component :is="p.backdrop" />
    </div>
  </template>

  <div v-show="visible" class="panels-wrapper" :class="{ dragging: deck.isDragging.value }">
    <template v-for="(p, i) in panels" :key="p.id">
      <div
        v-if="deck.isMounted(i)"
        class="panel"
        :class="[`${p.id}-panel`, { 'morph-parked': deck.isParked(i) }]"
        :style="deck.panelStyle(i)"
      >
        <component :is="p.component" />
      </div>
    </template>
  </div>

  <div v-show="visible" class="panel-indicator">
    <button
      v-for="(p, i) in panels"
      :key="p.id"
      class="panel-nav-btn"
      :class="{ active: deck.activePanel.value === i, 'has-badge': p.badge?.() }"
      :title="t(p.titleKey)"
      @click.stop="deck.switchPanel(i)"
    >
      <i :class="'fa-solid ' + p.icon"></i>
    </button>
    <div class="panel-indicator-sep"></div>
    <button class="panel-nav-btn" :title="t('openSettings')" @click.stop="settingsApi.open()">
      <i class="fa-solid fa-gear"></i>
    </button>
  </div>
</template>
