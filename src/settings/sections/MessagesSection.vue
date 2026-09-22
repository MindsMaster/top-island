<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { wechatApi } from '@/platform/wechat';
import { useI18n } from '@/core/i18n';
import { settings } from '@/core/settings';
import SettingSwitch from '@/ui/SettingSwitch.vue';

const { t } = useI18n();

const hasKey = ref(false);
const acquiring = ref(false);
const message = ref('');

async function acquireKey() {
  if (acquiring.value) return;
  acquiring.value = true;
  message.value = t('wechatAcquiring');
  const r = await wechatApi.acquireKey().catch(() => ({ ok: false, error: 'error' }));
  acquiring.value = false;
  hasKey.value = r.ok;
  if (r.ok) message.value = t('wechatKeyOk');
  else message.value = r.error === 'no-account' ? t('wechatNoAccount') : t('wechatKeyFail');
}

onMounted(async () => {
  hasKey.value = await wechatApi.hasKey().catch(() => false);
});
</script>

<template>
  <section class="setting-group">
    <SettingSwitch v-model="settings.notifications.enabled" :label="t('notifyEnable')" />
    <template v-if="settings.notifications.enabled">
      <SettingSwitch v-model="settings.notifications.popup" :label="t('notifyPopup')" />
      <SettingSwitch
        v-model="settings.notifications.suppressBanner"
        :label="t('notifySuppress')"
        :description="t('notifySuppressHint')"
      />
    </template>
  </section>

  <template v-if="settings.notifications.enabled">
    <section v-if="settings.notifications.popup" class="setting-section">
      <h2 class="setting-group-title">{{ t('settingsPrivacyTitle') }}</h2>
      <div class="setting-group">
        <SettingSwitch
          v-model="settings.notifications.privacy.enabled"
          :label="t('notifyPrivacy')"
          :description="t('notifyPrivacyHint')"
        />
        <div v-if="settings.notifications.privacy.enabled" class="setting-subgroup">
          <SettingSwitch
            v-model="settings.notifications.privacy.blurAvatar"
            :label="t('notifyPrivacyAvatar')"
          />
          <SettingSwitch v-model="settings.notifications.privacy.blurName" :label="t('notifyPrivacyName')" />
          <SettingSwitch
            v-model="settings.notifications.privacy.replaceBody"
            :label="t('notifyPrivacyBody')"
          />
          <div v-if="settings.notifications.privacy.replaceBody" class="setting-sub-input">
            <input
              v-model="settings.notifications.privacy.bodyText"
              class="setting-text-input"
              type="text"
              maxlength="40"
              :aria-label="t('settingsPrivateText')"
              :placeholder="t('notifyPrivateBody')"
            />
          </div>
        </div>
      </div>
    </section>

    <section class="setting-section">
      <h2 class="setting-group-title">{{ t('wechatTitle') }}</h2>
      <div class="setting-group">
        <SettingSwitch v-model="settings.notifications.wechat" :label="t('wechatEnable')" />
        <template v-if="settings.notifications.wechat">
          <div class="setting-row">
            <div class="setting-label">{{ t('wechatKey') }}</div>
            <span class="setting-status">{{ hasKey ? t('wechatKeyHave') : t('wechatKeyNone') }}</span>
            <button
              class="setting-action-btn"
              :title="t('wechatHint')"
              :disabled="acquiring"
              @click="acquireKey"
            >
              <i v-if="acquiring" class="fa-solid fa-spinner fa-spin" aria-hidden="true"></i>
              {{ acquiring ? t('wechatAcquiringBtn') : hasKey ? t('wechatReacquire') : t('wechatAcquire') }}
            </button>
          </div>
          <p v-if="message" class="setting-feedback" role="status">{{ message }}</p>
        </template>
      </div>
    </section>
  </template>
</template>
