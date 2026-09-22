<script setup lang="ts">
import { nextTick, ref } from 'vue';
import { storeApi } from '@/platform/store';
import { systemApi } from '@/platform/system';
import { useI18n } from '@/core/i18n';
import { settings } from '@/core/settings';
import SettingSwitch from '@/ui/SettingSwitch.vue';

const { t } = useI18n();

const armed = ref(false);
const busy = ref(false);
const error = ref('');
const cancelEl = ref<HTMLButtonElement | null>(null);

async function arm() {
  error.value = '';
  armed.value = true;
  await nextTick();
  cancelEl.value?.focus();
  cancelEl.value?.scrollIntoView({ block: 'nearest' });
}

async function confirm() {
  if (busy.value) return;
  busy.value = true;
  try {
    await storeApi.clear();
  } catch {
    error.value = t('settingsResetError');
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <section class="setting-group">
    <div class="setting-row">
      <div class="setting-label">{{ t('diagLogLabel') }}</div>
      <button class="setting-action-btn" @click="systemApi.revealDataDir()">
        {{ t('diagLogReveal') }}
      </button>
    </div>
    <SettingSwitch v-model="settings.diagnostics.devOverlay" :label="t('diagDevOverlay')" />
  </section>
  <section class="setting-section">
    <h2 class="setting-group-title">{{ t('settingsDataTitle') }}</h2>
    <div class="setting-group">
      <div class="setting-row">
        <div class="setting-label">{{ t('settingsResetLabel') }}</div>
        <button class="setting-action-btn danger" :disabled="armed" @click="arm">
          {{ t('settingsResetBtn') }}
        </button>
      </div>
      <div
        v-if="armed"
        class="reset-prompt"
        role="group"
        :aria-label="t('settingsResetTitle')"
        @keydown.esc.stop="armed = false"
      >
        <p>{{ t('settingsResetHint') }}</p>
        <p v-if="error" class="setting-error" role="alert">{{ error }}</p>
        <div class="reset-confirm">
          <button ref="cancelEl" class="setting-action-btn" :disabled="busy" @click="armed = false">
            {{ t('settingsResetCancel') }}
          </button>
          <button class="setting-action-btn danger" :disabled="busy" @click="confirm">
            {{ t('settingsResetConfirm') }}
          </button>
        </div>
      </div>
    </div>
  </section>
</template>
