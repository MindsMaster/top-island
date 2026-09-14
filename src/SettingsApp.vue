<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { api } from './api';
import { useI18n } from './i18n';
import { hexLuminance, THEMES, initSettings, setCustomColor, settings } from './store/settings';
import SettingSelect from './components/SettingSelect.vue';
import ColorPicker from './components/ColorPicker.vue';
import type {
  BridgeStatus,
  DiagnosticsToggles,
  DisplayInfo,
  LangPref,
  ThemeId,
  UpdateCheckResult,
} from '../shared/ipc';

const { t, initI18n } = useI18n();
/** 读直接渲染（reactive 自动追踪）；写也直接改字段，持久化/广播由 store 的 watch 统一处理 */
const st = settings;

type SectionId = 'general' | 'messages' | 'music' | 'diag';
const sections: Array<{ id: SectionId; icon: string; titleKey: string }> = [
  { id: 'general', icon: 'fa-sliders', titleKey: 'settingsGeneral' },
  { id: 'messages', icon: 'fa-comment-dots', titleKey: 'settingsMessages' },
  { id: 'music', icon: 'fa-music', titleKey: 'settingsMusic' },
  { id: 'diag', icon: 'fa-stethoscope', titleKey: 'settingsDiag' },
];
const active = ref<SectionId>('general');

const diagToggles: Array<{ key: keyof DiagnosticsToggles; nameKey: string }> = [
  { key: 'clipboardPoll', nameKey: 'diagClipboardPoll' },
  { key: 'musicPoll', nameKey: 'diagMusicPoll' },
  { key: 'devOverlay', nameKey: 'diagDevOverlay' },
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
  (hexLuminance(st.customTheme.a) + hexLuminance(st.customTheme.b)) / 2 > 0.55
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
  if (st.island.displayId === 'primary') {
    return displays.value.find((d) => d.primary)?.id ?? '';
  }
  return st.island.displayId;
});

function onDisplayPick(id: string) {
  const d = displays.value.find((x) => x.id === id);
  // 选中主显示器时存 'primary'（跨重插拔更稳）
  settings.island.displayId = d?.primary ? 'primary' : id;
}

const SCALE_MIN = 45;
const SCALE_MAX = 300;

function onScaleTyped(e: Event) {
  const el = e.target as HTMLInputElement;
  const v = parseInt(el.value.replace(/\D/g, ''));
  const next = Number.isFinite(v) ? Math.max(SCALE_MIN, Math.min(SCALE_MAX, v)) : settings.island.scale;
  settings.island.scale = next;
  el.value = String(next);
}

function onPeekInput(e: Event) {
  settings.island.hiddenPeek = parseInt((e.target as HTMLInputElement).value);
}

/** 每次打开设置窗 +1：根节点换 key 重建以重播进入动画（窗口常驻不销毁，Vue 不会自己重挂载） */
const enterKey = ref(0);
api.onSettingsOpened(() => {
  enterKey.value++;
});

const bridgeStatus = ref<BridgeStatus>('notDetected');
const BRIDGE_STATUS_KEY: Record<BridgeStatus, string> = {
  notDetected: 'bridgeStatusNotDetected',
  needsRestart: 'bridgeStatusNeedsRestart',
  installed: 'bridgeStatusInstalled',
  connecting: 'bridgeStatusConnecting',
  connected: 'bridgeStatusConnected',
};
const bridgeStatusText = computed(() => t(BRIDGE_STATUS_KEY[bridgeStatus.value]));

async function refreshBridgeStatus() {
  if (!settings.music.neteaseBridge) return;
  bridgeStatus.value = await api.musicBridgeStatus().catch(() => 'notDetected' as BridgeStatus);
}

const wechatHasKey = ref(false);
const wechatAcquiring = ref(false);
const wechatMsg = ref('');
const appVersionLabel = ref('');
const updateBusy = ref(false);
const updateMsg = ref('');
const updateCanInstall = ref(false);

async function refreshWechatKey() {
  wechatHasKey.value = await api.wechatHasKey().catch(() => false);
}

function applyUpdateResult(r: UpdateCheckResult) {
  updateCanInstall.value = r.status === 'downloaded';
  if (r.status === 'dev') updateMsg.value = t('settingsUpdateDev');
  else if (r.status === 'checking') updateMsg.value = t('settingsUpdateChecking');
  else if (r.status === 'not-available') updateMsg.value = t('settingsUpdateLatest');
  else if (r.status === 'available') updateMsg.value = t('settingsUpdateAvailable', r.version ?? '');
  else if (r.status === 'downloaded') updateMsg.value = t('settingsUpdateDownloaded', r.version ?? '');
  else if (r.status === 'error') updateMsg.value = r.message || t('settingsUpdateError');
  else updateMsg.value = '';
}

async function checkForUpdate() {
  if (updateCanInstall.value) {
    void api.installUpdate();
    return;
  }
  updateBusy.value = true;
  updateMsg.value = t('settingsUpdateChecking');
  try {
    applyUpdateResult(await api.checkUpdate());
  } catch (e) {
    applyUpdateResult({ status: 'error', message: e instanceof Error ? e.message : String(e) });
  } finally {
    updateBusy.value = false;
  }
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

let focusedAt = 0;
const FOCUS_BLUR_GRACE_MS = 500;

function onWindowFocus() {
  focusedAt = Date.now();
}

function onWindowBlur() {
  if (Date.now() - focusedAt < FOCUS_BLUR_GRACE_MS) return;
  api.closeSelf();
}

onMounted(async () => {
  await initI18n();
  await initSettings();
  document.title = t('settingsTitle');
  window.addEventListener('blur', onWindowBlur);
  window.addEventListener('focus', onWindowFocus);
  document.addEventListener('mousedown', onDocMouseDownPicker);
  displays.value = await api.displaysList().catch(() => []);
  await refreshWechatKey();
  const ver = await api.getVersion().catch(() => ({ version: '', gitHash: '', packaged: true }));
  appVersionLabel.value = ver.gitHash ? `${ver.version} (${ver.gitHash})` : ver.version;
  api.onUpdateDownloaded((info) => applyUpdateResult({ status: 'downloaded', version: info.version }));
  applyUpdateResult(await api.getUpdateStatus().catch(() => ({ status: ver.packaged ? 'checking' : 'dev' })));
  void refreshBridgeStatus();
  window.setInterval(() => void refreshBridgeStatus(), 2000);
});
</script>

<template>
  <div id="settings-window" :key="enterKey">
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
            <button
              class="setting-toggle"
              :class="{ on: st.autoLaunch }"
              @click="settings.autoLaunch = !settings.autoLaunch"
            >
              <span class="setting-toggle-knob"></span>
            </button>
          </div>
          <div class="setting-row">
            <div class="setting-label">{{ t('settingsVersion') }} · {{ appVersionLabel }}</div>
          </div>
          <div class="setting-row">
            <div class="setting-label">{{ t('settingsCheckUpdate') }}</div>
            <button class="setting-action-btn" :disabled="updateBusy" @click="checkForUpdate">
              <i
                class="fa-solid"
                :class="
                  updateBusy ? 'fa-spinner fa-spin' : updateCanInstall ? 'fa-rotate' : 'fa-cloud-arrow-down'
                "
              ></i>
              {{ updateCanInstall ? t('updateRestart') : t('settingsCheckUpdate') }}
            </button>
          </div>
          <div v-if="updateMsg" class="setting-hint">{{ updateMsg }}</div>
          <div class="setting-row">
            <div class="setting-label">{{ t('settingsLanguage') }}</div>
            <SettingSelect
              :model-value="st.lang"
              :options="langOptions"
              @update:model-value="settings.lang = $event as LangPref"
            />
          </div>
          <div class="setting-row">
            <div class="setting-label">{{ t('settingsTheme') }}</div>
            <SettingSelect
              :model-value="st.theme"
              :options="themeOptions"
              :group-labels="themeGroupLabels"
              @update:model-value="settings.theme = $event as ThemeId"
            />
          </div>
          <div v-if="st.theme === 'custom'" class="setting-row custom-palette">
            <!-- 迷你岛预览 -->
            <div class="custom-preview">
              <div
                class="custom-preview-pill"
                :style="{ background: `linear-gradient(135deg, ${st.customTheme.a}, ${st.customTheme.b})` }"
              >
                <i class="fa-solid fa-music" :style="{ color: st.customTheme.accent }"></i>
                <span :style="{ color: previewTextColor }">12:34</span>
              </div>
            </div>
            <div class="custom-swatches">
              <div v-for="p in customColorParts" :key="p.key" class="custom-color-item">
                <button
                  class="custom-swatch"
                  :class="{ open: openPicker === p.key }"
                  :style="{ background: st.customTheme[p.key] }"
                  @click="togglePicker(p.key)"
                ></button>
                <span class="custom-color-label">{{ t(p.labelKey) }}</span>
                <div v-if="openPicker === p.key" class="cp-popover">
                  <ColorPicker
                    :model-value="st.customTheme[p.key]"
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
                :value="st.island.scale"
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
              <span class="offset-value">{{ st.island.hiddenPeek }}px</span>
            </div>
            <div class="offset-control">
              <input
                type="range"
                class="offset-slider"
                min="2"
                max="20"
                step="1"
                :value="st.island.hiddenPeek"
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
              :class="{ on: st.notifications.enabled }"
              @click="settings.notifications.enabled = !settings.notifications.enabled"
            >
              <span class="setting-toggle-knob"></span>
            </button>
          </div>
          <div v-if="st.notifications.enabled" class="setting-row">
            <div class="setting-label">{{ t('notifyPopup') }}</div>
            <button
              class="setting-toggle"
              :class="{ on: st.notifications.popup }"
              @click="settings.notifications.popup = !settings.notifications.popup"
            >
              <span class="setting-toggle-knob"></span>
            </button>
          </div>
          <div v-if="st.notifications.enabled && st.notifications.popup" class="setting-row">
            <div class="setting-label">
              {{ t('notifyPrivacy') }}
              <span class="setting-help">
                <i class="fa-solid fa-circle-question"></i>
                <span class="setting-help-tip">{{ t('notifyPrivacyHint') }}</span>
              </span>
            </div>
            <button
              class="setting-toggle"
              :class="{ on: st.notifications.privacy.enabled }"
              @click="settings.notifications.privacy.enabled = !settings.notifications.privacy.enabled"
            >
              <span class="setting-toggle-knob"></span>
            </button>
          </div>
          <!-- 隐私细项：主开关开启后渐进披露 -->
          <div
            v-if="st.notifications.enabled && st.notifications.popup && st.notifications.privacy.enabled"
            class="setting-subgroup"
          >
            <div class="setting-row setting-sub">
              <div class="setting-label">{{ t('notifyPrivacyAvatar') }}</div>
              <button
                class="setting-toggle"
                :class="{ on: st.notifications.privacy.blurAvatar }"
                @click="
                  settings.notifications.privacy.blurAvatar = !settings.notifications.privacy.blurAvatar
                "
              >
                <span class="setting-toggle-knob"></span>
              </button>
            </div>
            <div class="setting-row setting-sub">
              <div class="setting-label">{{ t('notifyPrivacyName') }}</div>
              <button
                class="setting-toggle"
                :class="{ on: st.notifications.privacy.blurName }"
                @click="settings.notifications.privacy.blurName = !settings.notifications.privacy.blurName"
              >
                <span class="setting-toggle-knob"></span>
              </button>
            </div>
            <div class="setting-row setting-sub">
              <div class="setting-label">{{ t('notifyPrivacyBody') }}</div>
              <button
                class="setting-toggle"
                :class="{ on: st.notifications.privacy.replaceBody }"
                @click="
                  settings.notifications.privacy.replaceBody = !settings.notifications.privacy.replaceBody
                "
              >
                <span class="setting-toggle-knob"></span>
              </button>
            </div>
            <div v-if="st.notifications.privacy.replaceBody" class="setting-sub setting-sub-input">
              <input
                class="setting-text-input"
                type="text"
                maxlength="40"
                :value="st.notifications.privacy.bodyText"
                :placeholder="t('notifyPrivateBody')"
                @input="settings.notifications.privacy.bodyText = ($event.target as HTMLInputElement).value"
              />
            </div>
          </div>
          <template v-if="st.notifications.enabled">
            <div class="setting-row">
              <div class="setting-label">{{ t('notifySuppress') }}</div>
              <button
                class="setting-toggle"
                :class="{ on: st.notifications.suppressBanner }"
                @click="settings.notifications.suppressBanner = !settings.notifications.suppressBanner"
              >
                <span class="setting-toggle-knob"></span>
              </button>
            </div>

            <div class="setting-group-title wechat-divider">{{ t('wechatTitle') }}</div>
            <div class="setting-row">
              <div class="setting-label">{{ t('wechatEnable') }}</div>
              <button
                class="setting-toggle"
                :class="{ on: st.notifications.wechat }"
                @click="settings.notifications.wechat = !settings.notifications.wechat"
              >
                <span class="setting-toggle-knob"></span>
              </button>
            </div>
            <div v-if="st.notifications.wechat" class="setting-row">
              <div class="setting-label">
                {{ t('wechatKey') }}
                <span class="offset-value">{{ wechatHasKey ? t('wechatKeyHave') : t('wechatKeyNone') }}</span>
              </div>
              <button class="setting-action-btn" :disabled="wechatAcquiring" @click="acquireWechatKey">
                <i class="fa-solid" :class="wechatAcquiring ? 'fa-spinner fa-spin' : 'fa-key'"></i>
                {{ wechatHasKey ? t('wechatReacquire') : t('wechatAcquire') }}
              </button>
            </div>
            <div v-if="st.notifications.wechat && wechatMsg" class="setting-hint">{{ wechatMsg }}</div>
          </template>
        </template>

        <!-- 音乐 -->
        <template v-else-if="active === 'music'">
          <div class="setting-group-title">{{ t('settingsMusicTitle') }}</div>
          <div class="setting-hint">{{ t('settingsMusicHint') }}</div>
          <div class="setting-row">
            <div class="setting-label">
              {{ t('musicNeteaseLabel') }}
              <span v-if="st.music.neteaseBridge" class="offset-value">{{ bridgeStatusText }}</span>
            </div>
            <button
              class="setting-toggle"
              :class="{ on: st.music.neteaseBridge }"
              @click="settings.music.neteaseBridge = !settings.music.neteaseBridge"
            >
              <span class="setting-toggle-knob"></span>
            </button>
          </div>
        </template>

        <!-- 诊断 -->
        <template v-else-if="active === 'diag'">
          <div class="setting-group-title">{{ t('settingsDiagTitle') }}</div>
          <div class="setting-hint">{{ t('settingsDiagHint') }}</div>
          <div v-for="tg in diagToggles" :key="tg.key" class="setting-row">
            <div class="setting-label">{{ t(tg.nameKey) }}</div>
            <button
              class="setting-toggle"
              :class="{ on: st.diagnostics[tg.key] }"
              @click="settings.diagnostics[tg.key] = !settings.diagnostics[tg.key]"
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
              <button class="setting-action-btn" @click="resetArmed = false">
                {{ t('settingsResetCancel') }}
              </button>
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
