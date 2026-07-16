<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { api } from './api';
import { useI18n } from './i18n';
import { hexLuminance, THEMES, useSettings } from './composables/useSettings';
import SettingSelect from './components/SettingSelect.vue';
import ColorPicker from './components/ColorPicker.vue';
import type { DiagnosticsToggles, DisplayInfo, LangPref, ThemeId } from '../shared/ipc';

const { t, initI18n } = useI18n();
const {
  theme,
  customTheme,
  island,
  lang,
  notifications,
  diagnostics,
  autoLaunch,
  initSettings,
  setDiagnostic,
  setCustomColor,
  setIsland,
  setNotifications,
  setPrivacy,
} = useSettings();

type SectionId = 'general' | 'messages' | 'diag';
const sections: Array<{ id: SectionId; icon: string; titleKey: string }> = [
  { id: 'general', icon: 'fa-sliders', titleKey: 'settingsGeneral' },
  { id: 'messages', icon: 'fa-comment-dots', titleKey: 'settingsMessages' },
  { id: 'diag', icon: 'fa-stethoscope', titleKey: 'settingsDiag' },
];
const active = ref<SectionId>('general');

const diagToggles: Array<{ key: keyof DiagnosticsToggles; nameKey: string }> = [
  { key: 'clipboardPoll', nameKey: 'diagClipboardPoll' },
  { key: 'musicPoll', nameKey: 'diagMusicPoll' },
];

const themeOptions = computed(() =>
  THEMES.map((tm) => ({ value: tm.id, label: t(tm.nameKey), icon: tm.icon, group: tm.group }))
);
const themeGroupLabels = computed(() => ({
  solid: t('themeGroupSolid'),
  gradient: t('themeGroupGradient'),
  custom: t('themeGroupCustom'),
}));

/** 调色板：三个可自由搭配的颜色（改动即切到自定义主题实时预览） */
const customColorParts = computed(() => [
  { key: 'a' as const, labelKey: 'customColorA' },
  { key: 'b' as const, labelKey: 'customColorB' },
  { key: 'accent' as const, labelKey: 'customColorAccent' },
]);

/** 当前展开的取色器（一次只开一个） */
const openPicker = ref<null | 'a' | 'b' | 'accent'>(null);

function togglePicker(key: 'a' | 'b' | 'accent') {
  openPicker.value = openPicker.value === key ? null : key;
}

function onDocMouseDownPicker(e: MouseEvent) {
  if (!(e.target as HTMLElement).closest('.custom-color-item')) openPicker.value = null;
}

/** 迷你岛预览的文字颜色：按背景平均亮度取黑/白（与 applyTheme 同规则） */
const previewTextColor = computed(() =>
  (hexLuminance(customTheme.value.a) + hexLuminance(customTheme.value.b)) / 2 > 0.55
    ? 'rgba(0, 0, 0, 0.85)'
    : '#fff'
);

const langOptions = computed(() => [
  { value: 'auto', label: t('langAuto'), icon: 'fa-desktop' },
  { value: 'zh-CN', label: '简体中文', icon: 'fa-language' },
  { value: 'en-US', label: 'English', icon: 'fa-language' },
]);

const displays = ref<DisplayInfo[]>([]);

const displayOptions = computed(() => {
  const opts = displays.value.map((d) => ({
    value: d.id,
    label: d.primary ? `${t('displayPrimary')} · ${d.label}` : d.label,
    icon: 'fa-display',
  }));
  return opts;
});

const displayValue = computed(() => {
  if (island.value.displayId === 'primary') {
    return displays.value.find((d) => d.primary)?.id ?? '';
  }
  return island.value.displayId;
});

function onDisplayPick(id: string) {
  const d = displays.value.find((x) => x.id === id);
  // 选中主显示器时存 'primary'（跨重插拔更稳）
  setIsland({ displayId: d?.primary ? 'primary' : id });
}

const SCALE_MIN = 45;
const SCALE_MAX = 300;

function onScaleTyped(e: Event) {
  const el = e.target as HTMLInputElement;
  const v = parseInt(el.value.replace(/\D/g, ''));
  const next = Number.isFinite(v) ? Math.max(SCALE_MIN, Math.min(SCALE_MAX, v)) : island.value.scale;
  setIsland({ scale: next });
  el.value = String(next);
}

function onPeekInput(e: Event) {
  setIsland({ hiddenPeek: parseInt((e.target as HTMLInputElement).value) });
}

const wechatHasKey = ref(false);
const wechatAcquiring = ref(false);
const wechatMsg = ref('');

async function refreshWechatKey() {
  wechatHasKey.value = await api.wechatHasKey().catch(() => false);
}

async function acquireWechatKey() {
  if (wechatAcquiring.value) return;
  wechatAcquiring.value = true;
  wechatMsg.value = t('wechatAcquiring');
  const r = await api
    .wechatAcquireKey()
    .catch(() => ({ ok: false, error: 'error' }) as { ok: boolean; wxid?: string; error?: string });
  wechatAcquiring.value = false;
  if (r.ok) {
    wechatHasKey.value = true;
    wechatMsg.value = t('wechatKeyOk') + (r.wxid ? ` (${r.wxid})` : '');
  } else {
    wechatHasKey.value = false;
    wechatMsg.value = r.error === 'no-account' ? t('wechatNoAccount') : t('wechatKeyFail');
  }
}

// 二次确认防误触：清空全部本地数据并重启
const resetArmed = ref(false);
let resetTimer: number | null = null;

function armReset() {
  resetArmed.value = true;
  if (resetTimer) clearTimeout(resetTimer);
  resetTimer = window.setTimeout(() => (resetArmed.value = false), 4000);
}

function confirmReset() {
  if (resetTimer) clearTimeout(resetTimer);
  api.storeClear();
}

function onWindowBlur() {
  api.closeSelf();
}

onMounted(async () => {
  await initI18n();
  await initSettings();
  document.title = t('settingsTitle');
  window.addEventListener('blur', onWindowBlur);
  document.addEventListener('mousedown', onDocMouseDownPicker);
  displays.value = await api.displaysList().catch(() => []);
  await refreshWechatKey();
});
</script>

<template>
  <div id="settings-window">
    <header class="settings-header">
      <i class="fa-solid fa-gear settings-header-icon"></i>
      <span class="settings-title">{{ t('settingsTitle') }}</span>
      <button class="settings-close" :title="t('settingsClose')" @click="api.closeSelf()">
        <i class="fa-solid fa-xmark"></i>
      </button>
    </header>

    <div class="settings-body">
      <nav class="settings-nav">
        <button
          v-for="s in sections"
          :key="s.id"
          class="settings-nav-btn"
          :class="{ active: active === s.id }"
          @click="active = s.id"
        >
          <i :class="'fa-solid ' + s.icon"></i>
          <span>{{ t(s.titleKey) }}</span>
        </button>
      </nav>

      <main class="settings-content">
        <!-- 通用 -->
        <template v-if="active === 'general'">
          <div class="setting-row">
            <div class="setting-label">{{ t('settingsAutoLaunch') }}</div>
            <button class="setting-toggle" :class="{ on: autoLaunch }" @click="autoLaunch = !autoLaunch">
              <span class="setting-toggle-knob"></span>
            </button>
          </div>
          <div class="setting-row">
            <div class="setting-label">{{ t('settingsLanguage') }}</div>
            <SettingSelect
              :model-value="lang"
              :options="langOptions"
              @update:model-value="lang = $event as LangPref"
            />
          </div>
          <div class="setting-row">
            <div class="setting-label">{{ t('settingsTheme') }}</div>
            <SettingSelect
              :model-value="theme"
              :options="themeOptions"
              :group-labels="themeGroupLabels"
              @update:model-value="theme = $event as ThemeId"
            />
          </div>
          <div v-if="theme === 'custom'" class="setting-row custom-palette">
            <!-- 迷你岛预览 -->
            <div class="custom-preview">
              <div
                class="custom-preview-pill"
                :style="{ background: `linear-gradient(135deg, ${customTheme.a}, ${customTheme.b})` }"
              >
                <i class="fa-solid fa-music" :style="{ color: customTheme.accent }"></i>
                <span :style="{ color: previewTextColor }">12:34</span>
              </div>
            </div>
            <div class="custom-swatches">
              <div v-for="p in customColorParts" :key="p.key" class="custom-color-item">
                <button
                  class="custom-swatch"
                  :class="{ open: openPicker === p.key }"
                  :style="{ background: customTheme[p.key] }"
                  @click="togglePicker(p.key)"
                ></button>
                <span class="custom-color-label">{{ t(p.labelKey) }}</span>
                <div v-if="openPicker === p.key" class="cp-popover">
                  <ColorPicker
                    :model-value="customTheme[p.key]"
                    @update:model-value="setCustomColor(p.key, $event)"
                  />
                </div>
              </div>
            </div>
          </div>
          <div v-if="displays.length > 1" class="setting-row">
            <div class="setting-label">{{ t('settingsDisplayMonitor') }}</div>
            <SettingSelect
              :model-value="displayValue"
              :options="displayOptions"
              @update:model-value="onDisplayPick"
            />
          </div>
          <div class="setting-row">
            <div class="setting-label">{{ t('settingsIslandScale') }}</div>
            <div class="setting-num">
              <input
                class="setting-num-input"
                inputmode="numeric"
                maxlength="3"
                :value="island.scale"
                spellcheck="false"
                @focus="($event.target as HTMLInputElement).select()"
                @mouseup.prevent="($event.target as HTMLInputElement).select()"
                @keydown.enter="($event.target as HTMLInputElement).blur()"
                @blur="onScaleTyped"
              />
              <span class="setting-num-unit">%</span>
            </div>
          </div>
          <div class="setting-row offset-row">
            <div class="setting-label">
              {{ t('settingsHiddenPeek') }}
              <span class="offset-value">{{ island.hiddenPeek }}px</span>
            </div>
            <div class="offset-control">
              <input
                type="range"
                class="offset-slider"
                min="2"
                max="20"
                step="1"
                :value="island.hiddenPeek"
                @input="onPeekInput"
              />
            </div>
          </div>
        </template>

        <!-- 消息托管 -->
        <template v-else-if="active === 'messages'">
          <div class="setting-group-title">
            {{ t('settingsMessagesTitle') }}
            <span class="setting-help">
              <i class="fa-solid fa-circle-question"></i>
              <span class="setting-help-tip">{{ t('settingsMessagesHint') }}</span>
            </span>
          </div>
          <div class="setting-row">
            <div class="setting-label">{{ t('notifyEnable') }}</div>
            <button
              class="setting-toggle"
              :class="{ on: notifications.enabled }"
              @click="setNotifications({ enabled: !notifications.enabled })"
            >
              <span class="setting-toggle-knob"></span>
            </button>
          </div>
          <div v-if="notifications.enabled" class="setting-row">
            <div class="setting-label">{{ t('notifyPopup') }}</div>
            <button
              class="setting-toggle"
              :class="{ on: notifications.popup }"
              @click="setNotifications({ popup: !notifications.popup })"
            >
              <span class="setting-toggle-knob"></span>
            </button>
          </div>
          <div v-if="notifications.enabled && notifications.popup" class="setting-row">
            <div class="setting-label">
              {{ t('notifyPrivacy') }}
              <span class="setting-help">
                <i class="fa-solid fa-circle-question"></i>
                <span class="setting-help-tip">{{ t('notifyPrivacyHint') }}</span>
              </span>
            </div>
            <button
              class="setting-toggle"
              :class="{ on: notifications.privacy.enabled }"
              @click="setPrivacy({ enabled: !notifications.privacy.enabled })"
            >
              <span class="setting-toggle-knob"></span>
            </button>
          </div>
          <!-- 隐私细项：主开关开启后渐进披露 -->
          <div
            v-if="notifications.enabled && notifications.popup && notifications.privacy.enabled"
            class="setting-subgroup"
          >
            <div class="setting-row setting-sub">
              <div class="setting-label">{{ t('notifyPrivacyAvatar') }}</div>
              <button
                class="setting-toggle"
                :class="{ on: notifications.privacy.blurAvatar }"
                @click="setPrivacy({ blurAvatar: !notifications.privacy.blurAvatar })"
              >
                <span class="setting-toggle-knob"></span>
              </button>
            </div>
            <div class="setting-row setting-sub">
              <div class="setting-label">{{ t('notifyPrivacyName') }}</div>
              <button
                class="setting-toggle"
                :class="{ on: notifications.privacy.blurName }"
                @click="setPrivacy({ blurName: !notifications.privacy.blurName })"
              >
                <span class="setting-toggle-knob"></span>
              </button>
            </div>
            <div class="setting-row setting-sub">
              <div class="setting-label">{{ t('notifyPrivacyBody') }}</div>
              <button
                class="setting-toggle"
                :class="{ on: notifications.privacy.replaceBody }"
                @click="setPrivacy({ replaceBody: !notifications.privacy.replaceBody })"
              >
                <span class="setting-toggle-knob"></span>
              </button>
            </div>
            <div v-if="notifications.privacy.replaceBody" class="setting-sub setting-sub-input">
              <input
                class="setting-text-input"
                type="text"
                maxlength="40"
                :value="notifications.privacy.bodyText"
                :placeholder="t('notifyPrivateBody')"
                @input="setPrivacy({ bodyText: ($event.target as HTMLInputElement).value })"
              />
            </div>
          </div>
          <template v-if="notifications.enabled">
            <div class="setting-row">
              <div class="setting-label">{{ t('notifySuppress') }}</div>
              <button
                class="setting-toggle"
                :class="{ on: notifications.suppressBanner }"
                @click="setNotifications({ suppressBanner: !notifications.suppressBanner })"
              >
                <span class="setting-toggle-knob"></span>
              </button>
            </div>

            <div class="setting-group-title wechat-divider">{{ t('wechatTitle') }}</div>
            <div class="setting-row">
              <div class="setting-label">{{ t('wechatEnable') }}</div>
              <button
                class="setting-toggle"
                :class="{ on: notifications.wechat }"
                @click="setNotifications({ wechat: !notifications.wechat })"
              >
                <span class="setting-toggle-knob"></span>
              </button>
            </div>
            <div v-if="notifications.wechat" class="setting-row">
              <div class="setting-label">
                {{ t('wechatKey') }}
                <span class="offset-value">{{ wechatHasKey ? t('wechatKeyHave') : t('wechatKeyNone') }}</span>
              </div>
              <button class="setting-action-btn" :disabled="wechatAcquiring" @click="acquireWechatKey">
                <i class="fa-solid" :class="wechatAcquiring ? 'fa-spinner fa-spin' : 'fa-key'"></i>
                {{ wechatHasKey ? t('wechatReacquire') : t('wechatAcquire') }}
              </button>
            </div>
            <div v-if="notifications.wechat && wechatMsg" class="setting-hint">{{ wechatMsg }}</div>
          </template>
        </template>

        <!-- 诊断 -->
        <template v-else-if="active === 'diag'">
          <div class="setting-group-title">{{ t('settingsDiagTitle') }}</div>
          <div class="setting-hint">{{ t('settingsDiagHint') }}</div>
          <div v-for="tg in diagToggles" :key="tg.key" class="setting-row">
            <div class="setting-label">{{ t(tg.nameKey) }}</div>
            <button
              class="setting-toggle"
              :class="{ on: diagnostics[tg.key] }"
              @click="setDiagnostic(tg.key, !diagnostics[tg.key])"
            >
              <span class="setting-toggle-knob"></span>
            </button>
          </div>
          <div class="setting-row">
            <div class="setting-label">{{ t('diagLogLabel') }}</div>
            <button class="setting-action-btn" @click="api.diagReveal()">
              <i class="fa-solid fa-folder-open"></i>{{ t('diagLogReveal') }}
            </button>
          </div>

          <div class="setting-group-title reset-divider">{{ t('settingsResetTitle') }}</div>
          <div class="setting-hint">{{ t('settingsResetHint') }}</div>
          <div class="setting-row">
            <div class="setting-label">{{ t('settingsResetLabel') }}</div>
            <button v-if="!resetArmed" class="setting-action-btn danger" @click="armReset">
              <i class="fa-solid fa-trash-can"></i>{{ t('settingsResetBtn') }}
            </button>
            <div v-else class="reset-confirm">
              <button class="setting-action-btn" @click="resetArmed = false">{{ t('settingsResetCancel') }}</button>
              <button class="setting-action-btn danger" @click="confirmReset">
                <i class="fa-solid fa-triangle-exclamation"></i>{{ t('settingsResetConfirm') }}
              </button>
            </div>
          </div>
        </template>
      </main>
    </div>
  </div>
</template>
