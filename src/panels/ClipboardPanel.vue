<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from '../i18n';
import {
  clipboardState,
  filterClips,
  setSearch,
  readCurrent,
  clearAll,
  copy,
  deleteItem,
  togglePin,
  isPinned,
  formatTime,
  typeIcon,
  canOpenPath,
  openUrl,
} from '../store/clipboard';

const { t } = useI18n();
const snap = clipboardState;
const filtered = computed(() => filterClips(snap.history, snap.pinned, snap.search));

function onSearchInput(e: Event) {
  setSearch((e.target as HTMLInputElement).value);
}
</script>

<template>
  <div class="clip-header">
    <i class="fa-solid fa-clipboard-list clip-icon"></i>
    <span class="clip-title">{{ t('clipboardHeader') }}</span>
    <span class="clip-count">({{ snap.history.length }})</span>
    <button class="clip-refresh-btn" :title="t('clipboardRefresh')" @click.stop="readCurrent()">
      <i class="fa-solid fa-rotate"></i>
    </button>
    <button
      v-if="snap.history.length"
      class="clip-clear-btn"
      :title="t('clipboardClear')"
      @click.stop="clearAll()"
    >
      <i class="fa-solid fa-trash"></i>
    </button>
  </div>
  <div v-if="snap.history.length" class="clip-search-bar">
    <i class="fa-solid fa-search clip-search-icon"></i>
    <input
      :value="snap.search"
      type="text"
      class="clip-search-input"
      :placeholder="t('clipboardSearch')"
      @click.stop
      @input="onSearchInput"
    />
  </div>
  <div class="clip-list">
    <div v-if="!filtered.length" class="clip-empty">
      <i class="fa-solid fa-copy"></i>
      <span>{{ snap.history.length ? t('clipboardNoMatch') : t('clipboardEmpty') }}</span>
    </div>
    <div v-for="item in filtered" :key="item.id" class="clip-row">
      <div class="clip-type-icon" :class="'type-' + item.type">
        <i :class="typeIcon(item.type)"></i>
      </div>
      <div class="clip-content" @click.stop="copy(item.text)">
        <div class="clip-text">
          {{ (item.text || '').substring(0, 80) }}{{ (item.text || '').length > 80 ? '...' : '' }}
        </div>
        <div class="clip-meta">
          <span class="clip-time">{{ formatTime(item.time) }}</span>
        </div>
      </div>
      <div class="clip-actions">
        <button
          v-if="item.type === 'url'"
          class="clip-act-btn open"
          :title="t('clipboardOpenBrowser')"
          @click.stop="openUrl(item.text)"
        >
          <i class="fa-solid fa-arrow-up-right-from-square"></i>
        </button>
        <button
          v-else-if="['image', 'audio', 'video', 'file'].includes(item.type) && canOpenPath(item)"
          class="clip-act-btn open"
          :title="t('clipboardOpen')"
          @click.stop="openUrl(item.text)"
        >
          <i class="fa-solid fa-arrow-up-right-from-square"></i>
        </button>
        <button
          v-if="item.type === 'email'"
          class="clip-act-btn email"
          :title="t('clipboardCompose')"
          @click.stop="openUrl('mailto:' + item.text)"
        >
          <i class="fa-solid fa-paper-plane"></i>
        </button>
        <button class="clip-act-btn copy" :title="t('clipboardCopy')" @click.stop="copy(item.text)">
          <i class="fa-solid fa-copy"></i>
        </button>
        <button
          class="clip-act-btn pin"
          :class="{ active: isPinned(item.id) }"
          :title="t('clipboardPin')"
          @click.stop="togglePin(item.id)"
        >
          <i class="fa-solid fa-thumbtack"></i>
        </button>
        <button class="clip-act-btn del" :title="t('clipboardDelete')" @click.stop="deleteItem(item.id)">
          <i class="fa-solid fa-xmark"></i>
        </button>
      </div>
    </div>
  </div>
</template>
