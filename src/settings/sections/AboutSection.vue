<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { systemApi } from '@/platform/system';
import { updateApi } from '@/platform/update';
import { useI18n } from '@/core/i18n';
import appIcon from '@/assets/app-icon.png';
import type { UpdateCheckResult } from '@/platform/types';

const { t } = useI18n();

const versionLabel = ref('');
const busy = ref(false);
const message = ref('');
const canInstall = ref(false);

function show(r: UpdateCheckResult) {
  canInstall.value = r.status === 'downloaded';
  const texts: Record<UpdateCheckResult['status'], string> = {
    dev: t('settingsUpdateDev'),
    checking: t('settingsUpdateChecking'),
    'not-available': t('settingsUpdateLatest'),
    available: t('settingsUpdateAvailable', r.version ?? ''),
    downloaded: t('settingsUpdateDownloaded', r.version ?? ''),
    error: r.message || t('settingsUpdateError'),
  };
  message.value = texts[r.status] ?? '';
}

async function checkOrInstall() {
  if (canInstall.value) {
    void updateApi.install();
    return;
  }
  busy.value = true;
  message.value = t('settingsUpdateChecking');
  try {
    show(await updateApi.check());
  } catch (e) {
    show({ status: 'error', message: e instanceof Error ? e.message : String(e) });
  } finally {
    busy.value = false;
  }
}

onMounted(async () => {
  const ver = await systemApi.version().catch(() => ({ version: '', gitHash: '', packaged: true }));
  versionLabel.value = ver.gitHash ? `${ver.version} (${ver.gitHash})` : ver.version;
  updateApi.onDownloaded((info) => show({ status: 'downloaded', version: info.version }));
  show(await updateApi.status().catch(() => ({ status: ver.packaged ? 'checking' : 'dev' })));
});
</script>

<template>
  <div class="settings-app-identity">
    <img class="settings-app-icon" :src="appIcon" alt="" />
    <div>
      <h2>Top Island</h2>
      <p>{{ versionLabel }}</p>
    </div>
  </div>
  <section class="setting-group">
    <div class="setting-row">
      <div class="setting-label" :title="message" role="status">
        {{ message || t('settingsUpdateTitle') }}
      </div>
      <button class="setting-action-btn" :disabled="busy" @click="checkOrInstall">
        <i v-if="busy" class="fa-solid fa-spinner fa-spin" aria-hidden="true"></i>
        {{ canInstall ? t('updateRestart') : t('settingsCheckUpdate') }}
      </button>
    </div>
  </section>
</template>
