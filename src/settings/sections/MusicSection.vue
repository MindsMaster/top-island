<script setup lang="ts">
import { onBeforeUnmount, onMounted, reactive } from 'vue';
import { musicApi } from '@/platform/music';
import { useI18n } from '@/core/i18n';
import { settings } from '@/core/settings';
import SettingSwitch from '@/ui/SettingSwitch.vue';
import type { GrantResult, IntegrationStatus, MusicConfig } from '@/platform/types';

const { t } = useI18n();

interface IntegrationEntry {
  /** 与 Rust Integration::id 一致 */
  id: string;
  setting: keyof MusicConfig;
  labelKey: string;
  hintKey: string;
}

const INTEGRATIONS: IntegrationEntry[] = [
  { id: 'netease', setting: 'neteaseBridge', labelKey: 'musicNeteaseLabel', hintKey: 'settingsMusicHint' },
  { id: 'kugou', setting: 'kugouBridge', labelKey: 'musicKugouLabel', hintKey: 'settingsMusicKugouHint' },
  { id: 'qqmusic', setting: 'qqmusicBridge', labelKey: 'musicQqLabel', hintKey: 'settingsMusicQqHint' },
];

const STATUS_KEY: Record<IntegrationStatus, string> = {
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

const statuses = reactive<Record<string, IntegrationStatus>>({});
const granting = reactive<Record<string, boolean>>({});
const grantResults = reactive<Record<string, GrantResult | undefined>>({});

const statusOf = (id: string) => statuses[id] ?? 'notDetected';

// 授权后保留按钮行 直到部署走完
const showGrant = (id: string) =>
  statusOf(id) === 'noPermission' || granting[id] || grantResults[id] === 'granted';

let timer: number | undefined;

async function refresh() {
  if (!INTEGRATIONS.some((i) => settings.music[i.setting])) return;
  const list = await musicApi.integrations().catch(() => []);
  for (const { id, status } of list) {
    statuses[id] = status;
    if (grantResults[id] === 'granted' && status !== 'noPermission') grantResults[id] = undefined;
  }
}

async function grant(id: string) {
  if (granting[id]) return;
  granting[id] = true;
  grantResults[id] = await musicApi.integrationGrant(id).catch(() => 'failed' as GrantResult);
  granting[id] = false;
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
    <div v-for="entry in INTEGRATIONS" :key="entry.id" class="setting-group">
      <SettingSwitch
        v-model="settings.music[entry.setting]"
        :label="t(entry.labelKey)"
        :description="t(entry.hintKey)"
      >
        <span
          v-if="settings.music[entry.setting]"
          class="setting-status"
          :title="t(STATUS_KEY[statusOf(entry.id)])"
          role="status"
          >{{ t(STATUS_KEY[statusOf(entry.id)]) }}</span
        >
      </SettingSwitch>
      <template v-if="settings.music[entry.setting] && showGrant(entry.id)">
        <div class="setting-row">
          <div class="setting-label">{{ t('bridgeGrantHint') }}</div>
          <button
            class="setting-action-btn"
            :disabled="granting[entry.id] || grantResults[entry.id] === 'granted'"
            @click="grant(entry.id)"
          >
            <i v-if="granting[entry.id]" class="fa-solid fa-spinner fa-spin" aria-hidden="true"></i>
            {{ granting[entry.id] ? t('bridgeGranting') : t('bridgeGrant') }}
          </button>
        </div>
        <p v-if="grantResults[entry.id]" class="setting-feedback" role="status">
          {{ t(GRANT_KEY[grantResults[entry.id]!]) }}
        </p>
      </template>
    </div>
  </section>
</template>
