<script setup lang="ts">
import { useI18n } from '../i18n';
import { openUrl, useClipboard } from '../composables/useClipboard';

const { t } = useI18n();
const clip = useClipboard();
</script>

<template>
  <div class="clip-header">
    <i class="fa-solid fa-clipboard-list clip-icon"></i>
    <span class="clip-title">{{ t('clipboardHeader') }}</span>
    <span class="clip-count">({{ clip.history.value.length }})</span>
    <button class="clip-refresh-btn" :title="t('clipboardRefresh')" @click.stop="clip.readCurrent()">
      <i class="fa-solid fa-rotate"></i>
    </button>
    <button
      v-if="clip.history.value.length"
      class="clip-clear-btn"
      :title="t('clipboardClear')"
      @click.stop="clip.clearAll()"
    >
      <i class="fa-solid fa-trash"></i>
    </button>
  </div>
  <div v-if="clip.history.value.length" class="clip-search-bar">
    <i class="fa-solid fa-search clip-search-icon"></i>
    <input
      v-model="clip.search.value"
      type="text"
      class="clip-search-input"
      :placeholder="t('clipboardSearch')"
      @click.stop
    />
  </div>
  <div class="clip-list">
    <div v-if="!clip.filtered.value.length" class="clip-empty">
      <i class="fa-solid fa-copy"></i>
      <span>{{ clip.history.value.length ? t('clipboardNoMatch') : t('clipboardEmpty') }}</span>
    </div>
    <div v-for="item in clip.filtered.value" :key="item.id" class="clip-row">
      <div class="clip-type-icon" :class="'type-' + item.type">
        <i :class="clip.typeIcon(item.type)"></i>
      </div>
      <div class="clip-content" @click.stop="clip.copy(item.text)">
        <div class="clip-text">
          {{ (item.text || '').substring(0, 80) }}{{ (item.text || '').length > 80 ? '...' : '' }}
        </div>
        <div class="clip-meta">
          <span class="clip-time">{{ clip.formatTime(item.time) }}</span>
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
          v-else-if="['image', 'audio', 'video', 'file'].includes(item.type) && clip.canOpenPath(item)"
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
        <button class="clip-act-btn copy" :title="t('clipboardCopy')" @click.stop="clip.copy(item.text)">
          <i class="fa-solid fa-copy"></i>
        </button>
        <button
          class="clip-act-btn pin"
          :class="{ active: clip.isPinned(item.id) }"
          :title="t('clipboardPin')"
          @click.stop="clip.togglePin(item.id)"
        >
          <i class="fa-solid fa-thumbtack"></i>
        </button>
        <button class="clip-act-btn del" :title="t('clipboardDelete')" @click.stop="clip.deleteItem(item.id)">
          <i class="fa-solid fa-xmark"></i>
        </button>
      </div>
    </div>
  </div>
</template>
