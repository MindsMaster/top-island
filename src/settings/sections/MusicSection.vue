<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { musicApi } from '@/platform/music';
import { useI18n } from '@/core/i18n';
import { settings } from '@/core/settings';
import SettingSwitch from '@/ui/SettingSwitch.vue';
import type { BridgeStatus, GrantResult } from '@/platform/types';

const { t } = useI18n();

const STATUS_KEY: Record<BridgeStatus, string> = {
  notDetected: 'bridgeStatusNotDetected',
  needsRestart: 'bridgeStatusNeedsRestart',
  installed: 'bridgeStatusInstalled',
  connecting: 'bridgeStatusConnecting',
  connected: 'bridgeStatusConnected',
  noPermission: 'bridgeStatusNoPermission',
  conflict: 'bridgeStatusConflict',
  unsupported: 'bridgeStatusUnsupported',
  failed: 'bridgeStatusFailed',
};

const GRANT_KEY: Record<GrantResult, string> = {
  granted: 'bridgeGrantOk',
  cancelled: 'bridgeGrantCancelled',
  failed: 'bridgeGrantFail',
};

const POLL_MS = 2000;

const status = ref<BridgeStatus>('notDetected');
const statusText = computed(() => t(STATUS_KEY[status.value]));

const granting = ref(false);
const grantResult = ref<GrantResult | null>(null);
// 授权后保留按钮行 直到部署走完
const showGrant = computed(
  () => status.value === 'noPermission' || granting.value || grantResult.value === 'granted'
);

let timer: number | undefined;

async function refresh() {
  if (!settings.music.neteaseBridge) return;
  status.value = await musicApi.bridgeStatus().catch(() => 'notDetected' as BridgeStatus);
  if (grantResult.value === 'granted' && status.value !== 'noPermission') grantResult.value = null;
}

async function grant() {
  if (granting.value) return;
  granting.value = true;
  grantResult.value = await musicApi.bridgeGrant().catch(() => 'failed' as GrantResult);
  granting.value = false;
  void refresh();
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
      <template v-if="settings.music.neteaseBridge && showGrant">
        <div class="setting-row">
          <div class="setting-label">{{ t('bridgeGrantHint') }}</div>
          <button class="setting-action-btn" :disabled="granting || grantResult === 'granted'" @click="grant">
            <i v-if="granting" class="fa-solid fa-spinner fa-spin" aria-hidden="true"></i>
            {{ granting ? t('bridgeGranting') : t('bridgeGrant') }}
          </button>
        </div>
        <p v-if="grantResult" class="setting-feedback" role="status">{{ t(GRANT_KEY[grantResult]) }}</p>
      </template>
    </div>
  </section>
</template>
