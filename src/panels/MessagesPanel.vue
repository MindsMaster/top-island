<script setup lang="ts">
import { useI18n } from '../i18n';
import { useNotifications } from '../composables/useNotifications';

const { t } = useI18n();
const notify = useNotifications();
</script>

<template>
  <div class="msg-header">
    <i class="fa-solid fa-comment-dots msg-icon"></i>
    <span class="msg-title">{{ t('messagesHeader') }}</span>
    <span class="msg-count">({{ notify.items.value.length }})</span>
    <button
      v-if="notify.items.value.length"
      class="msg-clear-btn"
      :title="t('messagesClear')"
      @click.stop="notify.clearAll()"
    >
      <i class="fa-solid fa-trash"></i>
    </button>
  </div>
  <div class="msg-list">
    <div v-if="!notify.items.value.length" class="msg-empty">
      <i class="fa-solid fa-bell-slash"></i>
      <span>{{ t('messagesEmpty') }}</span>
    </div>
    <div
      v-for="item in notify.items.value"
      :key="item.key"
      class="msg-row"
      @click.stop="notify.activate(item)"
    >
      <div class="msg-avatar">
        <img
          v-if="notify.images.value[item.key]"
          :src="notify.images.value[item.key]"
          alt=""
          draggable="false"
        />
        <span v-else class="msg-avatar-fallback">{{ (item.app || '?').slice(0, 1) }}</span>
      </div>
      <div class="msg-content">
        <div class="msg-top">
          <span class="msg-app">{{ item.app }}</span>
          <span class="msg-time">{{ notify.formatTime(item.arrival) }}</span>
        </div>
        <div v-if="item.title" class="msg-text-title">{{ item.title }}</div>
        <div v-if="item.body" class="msg-text-body">{{ item.body }}</div>
      </div>
      <button class="msg-del" :title="t('messagesDelete')" @click.stop="notify.remove(item.key)">
        <i class="fa-solid fa-xmark"></i>
      </button>
    </div>
  </div>
</template>
