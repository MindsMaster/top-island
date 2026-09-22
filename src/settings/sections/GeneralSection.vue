<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from '@/core/i18n';
import { settings } from '@/core/settings';
import SettingSelect from '@/ui/SettingSelect.vue';
import SettingSwitch from '@/ui/SettingSwitch.vue';
import type { LangPref } from '@/platform/types';

const { t } = useI18n();

const langOptions = computed(() => [
  { value: 'auto', label: t('langAuto'), icon: 'fa-desktop' },
  { value: 'zh-CN', label: '简体中文', icon: 'fa-language' },
  { value: 'en-US', label: 'English', icon: 'fa-language' },
]);
</script>

<template>
  <section class="setting-group">
    <SettingSwitch v-model="settings.autoLaunch" :label="t('settingsAutoLaunch')" />
    <div class="setting-row">
      <label id="language-label" class="setting-label">{{ t('settingsLanguage') }}</label>
      <SettingSelect
        :model-value="settings.lang"
        :options="langOptions"
        labelled-by="language-label"
        @update:model-value="settings.lang = $event as LangPref"
      />
    </div>
  </section>
  <section class="setting-section">
    <h2 class="setting-group-title">{{ t('settingsClipboardTitle') }}</h2>
    <div class="setting-group">
      <SettingSwitch v-model="settings.diagnostics.clipboardPoll" :label="t('diagClipboardPoll')" />
    </div>
  </section>
</template>
