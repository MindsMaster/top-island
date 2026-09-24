<script setup lang="ts">
import { useI18n } from '@/core/i18n';
import { notifyState, activate, clearAll, remove, formatTime, imageFailed } from './store';

const { t } = useI18n();
const snap = notifyState;
</script>

<template>
  <div class="msg-header">
    <i class="fa-solid fa-comment-dots msg-icon"></i>
    <span class="msg-title">{{ t('messagesHeader') }}</span>
    <span class="msg-count">({{ snap.items.length }})</span>
    <button
      v-if="snap.items.length"
      class="msg-clear-btn"
      :title="t('messagesClear')"
      @click.stop="clearAll()"
    >
      <i class="fa-solid fa-trash"></i>
    </button>
  </div>
  <div class="msg-list">
    <div v-if="!snap.items.length" class="msg-empty">
      <i class="fa-solid fa-bell-slash"></i>
      <span>{{ t('messagesEmpty') }}</span>
    </div>
    <div v-for="item in snap.items" :key="item.key" class="msg-row" @click.stop="activate(item)">
      <div class="msg-avatar">
        <img
          v-if="snap.images[item.key]"
          :src="snap.images[item.key]"
          alt=""
          draggable="false"
          @error="imageFailed(item, $event)"
        />
        <span v-else class="msg-avatar-fallback">{{ (item.app || '?').slice(0, 1) }}</span>
      </div>
      <div class="msg-content">
        <div class="msg-top">
          <span class="msg-app">{{ item.app }}</span>
          <span class="msg-time">{{ formatTime(item.arrival) }}</span>
        </div>
        <div v-if="item.title" class="msg-text-title">{{ item.title }}</div>
        <div v-if="item.body" class="msg-text-body">{{ item.body }}</div>
      </div>
      <button class="msg-del" :title="t('messagesDelete')" @click.stop="remove(item.key)">
        <i class="fa-solid fa-xmark"></i>
      </button>
    </div>
  </div>
</template>
