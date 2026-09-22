<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { musicApi } from '@/platform/music';
import { useI18n } from '@/core/i18n';
import { settings } from '@/core/settings';
import SettingSwitch from '@/ui/SettingSwitch.vue';
import type { BridgeStatus } from '@/platform/types';

const { t } = useI18n();

const STATUS_KEY: Record<BridgeStatus, string> = {
  notDetected: 'bridgeStatusNotDetected',
  needsRestart: 'bridgeStatusNeedsRestart',
  installed: 'bridgeStatusInstalled',
  connecting: 'bridgeStatusConnecting',
  connected: 'bridgeStatusConnected',
};

const POLL_MS = 2000;

const status = ref<BridgeStatus>('notDetected');
const statusText = computed(() => t(STATUS_KEY[status.value]));

let timer: number | undefined;

async function refresh() {
  if (!settings.music.neteaseBridge) return;
  status.value = await musicApi.bridgeStatus().catch(() => 'notDetected' as BridgeStatus);
}

onMounted(() => {
  void refresh();
  timer = window.setInterval(() => void refresh(), POLL_MS);
});

onBeforeUnmount(() => window.clearInterval(timer));
</script>

<template>
  <section class="setting-group">
    <SettingSwitch v-model="settings.diagnostics.musicPoll" :label="t('diagMusicPoll')" />
  </section>
  <section class="setting-section">
    <h2 class="setting-group-title">{{ t('settingsMusicTitle') }}</h2>
    <div class="setting-group">
      <SettingSwitch
        v-model="settings.music.neteaseBridge"
        :label="t('musicNeteaseLabel')"
        :description="t('settingsMusicHint')"
      >
        <span v-if="settings.music.neteaseBridge" class="setting-status" :title="statusText" role="status">{{
          statusText
        }}</span>
      </SettingSwitch>
    </div>
  </section>
</template>
