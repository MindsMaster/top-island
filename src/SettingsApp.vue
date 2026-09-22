<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { musicApi } from '@/platform/music';
import { settingsApi } from '@/platform/settings';
import { storeApi } from '@/platform/store';
import { systemApi } from '@/platform/system';
import { updateApi } from '@/platform/update';
import { wechatApi } from '@/platform/wechat';
import { windowApi } from '@/platform/window';
import { useI18n } from './i18n';
import { hexLuminance, THEMES, initSettings, setCustomColor, settings } from './store/settings';
import SettingSelect from './components/SettingSelect.vue';
import ColorPicker from './components/ColorPicker.vue';
import SettingSwitch from './components/SettingSwitch.vue';
import appIcon from './assets/app-icon.png';
import type { BridgeStatus, DisplayInfo, LangPref, ThemeId, UpdateCheckResult } from '@/platform/types';

const { t, initI18n } = useI18n();
/** 读直接渲染（reactive 自动追踪）；写也直接改字段，持久化/广播由 store 的 watch 统一处理 */
const st = settings;

type SectionId = 'general' | 'appearance' | 'messages' | 'music' | 'diag' | 'about';
const sections: Array<{ id: SectionId; icon: string; titleKey: string }> = [
  { id: 'general', icon: 'fa-sliders', titleKey: 'settingsGeneral' },
  { id: 'appearance', icon: 'fa-palette', titleKey: 'settingsAppearance' },
  { id: 'messages', icon: 'fa-bell', titleKey: 'settingsMessages' },
  { id: 'music', icon: 'fa-music', titleKey: 'settingsMusic' },
  { id: 'diag', icon: 'fa-gear', titleKey: 'settingsDiag' },
  { id: 'about', icon: 'fa-circle-info', titleKey: 'settingsAbout' },
];
const active = ref<SectionId>('general');
const activeTitle = computed(() => t(sections.find((section) => section.id === active.value)!.titleKey));
const contentEl = ref<HTMLElement | null>(null);

watch(active, async () => {
  openPicker.value = null;
  resetArmed.value = false;
  await nextTick();
  contentEl.value?.scrollTo(0, 0);
});

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
  const counts = new Map<string, number>();
  for (const d of displays.value) counts.set(d.label, (counts.get(d.label) ?? 0) + 1);
  // 同型号双屏友好名相同，用 GDI 名（DISPLAY2）区分
  return displays.value.map((d) => ({
    value: d.id,
    label: counts.get(d.label)! > 1 ? `${d.label} (${d.id.replace(/^\\+\.\\/, '')})` : d.label,
    icon: 'fa-display',
  }));
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
settingsApi.onOpened(() => {
  resetArmed.value = false;
  openPicker.value = null;
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
  bridgeStatus.value = await musicApi.bridgeStatus().catch(() => 'notDetected' as BridgeStatus);
}

const wechatHasKey = ref(false);
const wechatAcquiring = ref(false);
const wechatMsg = ref('');
const appVersionLabel = ref('');
const updateBusy = ref(false);
const updateMsg = ref('');
const updateCanInstall = ref(false);

async function refreshWechatKey() {
  wechatHasKey.value = await wechatApi.hasKey().catch(() => false);
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
    void updateApi.install();
    return;
  }
  updateBusy.value = true;
  updateMsg.value = t('settingsUpdateChecking');
  try {
    applyUpdateResult(await updateApi.check());
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
  const r = await wechatApi.acquireKey().catch(() => ({ ok: false, error: 'error' }));
  wechatAcquiring.value = false;
  if (r.ok) {
    wechatHasKey.value = true;
    wechatMsg.value = t('wechatKeyOk');
  } else {
    wechatHasKey.value = false;
    wechatMsg.value = r.error === 'no-account' ? t('wechatNoAccount') : t('wechatKeyFail');
  }
}

const resetArmed = ref(false);
const resetBusy = ref(false);
const resetError = ref('');
const resetCancelEl = ref<HTMLButtonElement | null>(null);

async function armReset() {
  resetError.value = '';
  resetArmed.value = true;
  await nextTick();
  resetCancelEl.value?.focus();
  resetCancelEl.value?.scrollIntoView({ block: 'nearest' });
}

async function confirmReset() {
  if (resetBusy.value) return;
  resetBusy.value = true;
  try {
    await storeApi.clear();
  } catch {
    resetError.value = t('settingsResetError');
  } finally {
    resetBusy.value = false;
  }
}

let focusedAt = 0;
const FOCUS_BLUR_GRACE_MS = 500;

function onWindowFocus() {
  focusedAt = Date.now();
}

function onWindowBlur() {
  if (Date.now() - focusedAt < FOCUS_BLUR_GRACE_MS) return;
  windowApi.closeSelf();
}

let bridgeTimer: number | undefined;

onBeforeUnmount(() => {
  window.removeEventListener('blur', onWindowBlur);
  window.removeEventListener('focus', onWindowFocus);
  document.removeEventListener('mousedown', onDocMouseDownPicker);
  window.clearInterval(bridgeTimer);
});

onMounted(async () => {
  await initI18n();
  await initSettings();
  document.title = t('settingsTitle');
  window.addEventListener('blur', onWindowBlur);
  window.addEventListener('focus', onWindowFocus);
  document.addEventListener('mousedown', onDocMouseDownPicker);
  displays.value = await systemApi.displays().catch(() => []);
  await refreshWechatKey();
  const ver = await systemApi.version().catch(() => ({ version: '', gitHash: '', packaged: true }));
  appVersionLabel.value = ver.gitHash ? `${ver.version} (${ver.gitHash})` : ver.version;
  updateApi.onDownloaded((info) => applyUpdateResult({ status: 'downloaded', version: info.version }));
  applyUpdateResult(await updateApi.status().catch(() => ({ status: ver.packaged ? 'checking' : 'dev' })));
  void refreshBridgeStatus();
  bridgeTimer = window.setInterval(() => void refreshBridgeStatus(), 2000);
});
</script>

<template>
  <div id="settings-window" :key="enterKey">
    <header class="settings-header">
      <span class="settings-title">{{ t('settingsTitle') }}</span>
      <button class="settings-close" :aria-label="t('settingsClose')" @click="windowApi.closeSelf()">
        <i class="fa-solid fa-xmark" aria-hidden="true"></i>
      </button>
    </header>

    <div class="settings-body">
      <nav class="settings-nav" :aria-label="t('settingsTitle')">
        <button
          v-for="s in sections"
          :key="s.id"
          class="settings-nav-btn"
          :class="{ active: active === s.id, 'settings-nav-bottom': s.id === 'about' }"
          :aria-current="active === s.id ? 'page' : undefined"
          @click="active = s.id"
        >
          <i :class="'fa-solid ' + s.icon" aria-hidden="true"></i>
          <span>{{ t(s.titleKey) }}</span>
        </button>
      </nav>

      <main ref="contentEl" class="settings-content" aria-labelledby="settings-page-title">
        <h1 id="settings-page-title" class="settings-page-title">{{ activeTitle }}</h1>

        <template v-if="active === 'general'">
          <section class="setting-group">
            <SettingSwitch v-model="st.autoLaunch" :label="t('settingsAutoLaunch')" />
            <div class="setting-row">
              <label id="language-label" class="setting-label">{{ t('settingsLanguage') }}</label>
              <SettingSelect
                :model-value="st.lang"
                :options="langOptions"
                labelled-by="language-label"
                @update:model-value="settings.lang = $event as LangPref"
              />
            </div>
          </section>
          <section class="setting-section">
            <h2 class="setting-group-title">{{ t('settingsClipboardTitle') }}</h2>
            <div class="setting-group">
              <SettingSwitch v-model="st.diagnostics.clipboardPoll" :label="t('diagClipboardPoll')" />
            </div>
          </section>
        </template>

        <template v-else-if="active === 'appearance'">
          <section class="setting-group">
            <div class="setting-row">
              <label id="theme-label" class="setting-label">{{ t('settingsTheme') }}</label>
              <SettingSelect
                :model-value="st.theme"
                :options="themeOptions"
                :group-labels="themeGroupLabels"
                labelled-by="theme-label"
                @update:model-value="settings.theme = $event as ThemeId"
              />
            </div>
            <div v-if="st.theme === 'custom'" class="setting-row custom-palette">
              <div class="custom-preview" aria-hidden="true">
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
                    :aria-label="t(p.labelKey)"
                    :aria-expanded="openPicker === p.key"
                    @click="togglePicker(p.key)"
                  ></button>
                  <span class="custom-color-label">{{ t(p.labelKey) }}</span>
                  <div v-if="openPicker === p.key" class="cp-popover" @keydown.esc.stop="openPicker = null">
                    <ColorPicker
                      :model-value="st.customTheme[p.key]"
                      @update:model-value="setCustomColor(p.key, $event)"
                    />
                  </div>
                </div>
              </div>
            </div>
          </section>
          <section class="setting-section">
            <h2 class="setting-group-title">{{ t('settingsLayoutTitle') }}</h2>
            <div class="setting-group">
              <div v-if="displays.length > 1" class="setting-row">
                <label id="display-label" class="setting-label">{{ t('settingsDisplayMonitor') }}</label>
                <SettingSelect
                  :model-value="displayValue"
                  :options="displayOptions"
                  labelled-by="display-label"
                  @update:model-value="onDisplayPick"
                />
              </div>
              <div class="setting-row">
                <label for="island-scale" class="setting-label">{{ t('settingsIslandScale') }}</label>
                <div class="setting-num">
                  <input
                    id="island-scale"
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
                <label for="hidden-peek" class="setting-label">
                  {{ t('settingsHiddenPeek') }}
                </label>
                <div class="offset-control">
                  <input
                    id="hidden-peek"
                    type="range"
                    class="offset-slider"
                    min="2"
                    max="20"
                    step="1"
                    :value="st.island.hiddenPeek"
                    @input="onPeekInput"
                  />
                  <span class="offset-value">{{ st.island.hiddenPeek }} px</span>
                </div>
              </div>
            </div>
          </section>
        </template>

        <template v-else-if="active === 'messages'">
          <section class="setting-group">
            <SettingSwitch v-model="st.notifications.enabled" :label="t('notifyEnable')" />
            <template v-if="st.notifications.enabled">
              <SettingSwitch v-model="st.notifications.popup" :label="t('notifyPopup')" />
              <SettingSwitch
                v-model="st.notifications.suppressBanner"
                :label="t('notifySuppress')"
                :description="t('notifySuppressHint')"
              />
            </template>
          </section>
          <template v-if="st.notifications.enabled">
            <section v-if="st.notifications.popup" class="setting-section">
              <h2 class="setting-group-title">{{ t('settingsPrivacyTitle') }}</h2>
              <div class="setting-group">
                <SettingSwitch
                  v-model="st.notifications.privacy.enabled"
                  :label="t('notifyPrivacy')"
                  :description="t('notifyPrivacyHint')"
                />
                <div v-if="st.notifications.privacy.enabled" class="setting-subgroup">
                  <SettingSwitch
                    v-model="st.notifications.privacy.blurAvatar"
                    :label="t('notifyPrivacyAvatar')"
                  />
                  <SettingSwitch
                    v-model="st.notifications.privacy.blurName"
                    :label="t('notifyPrivacyName')"
                  />
                  <SettingSwitch
                    v-model="st.notifications.privacy.replaceBody"
                    :label="t('notifyPrivacyBody')"
                  />
                  <div v-if="st.notifications.privacy.replaceBody" class="setting-sub-input">
                    <input
                      v-model="st.notifications.privacy.bodyText"
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
                <SettingSwitch v-model="st.notifications.wechat" :label="t('wechatEnable')" />
                <template v-if="st.notifications.wechat">
                  <div class="setting-row">
                    <div class="setting-label">{{ t('wechatKey') }}</div>
                    <span class="setting-status">{{
                      wechatHasKey ? t('wechatKeyHave') : t('wechatKeyNone')
                    }}</span>
                    <button
                      class="setting-action-btn"
                      :title="t('wechatHint')"
                      :disabled="wechatAcquiring"
                      @click="acquireWechatKey"
                    >
                      <i v-if="wechatAcquiring" class="fa-solid fa-spinner fa-spin" aria-hidden="true"></i>
                      {{
                        wechatAcquiring
                          ? t('wechatAcquiringBtn')
                          : wechatHasKey
                            ? t('wechatReacquire')
                            : t('wechatAcquire')
                      }}
                    </button>
                  </div>
                  <p v-if="wechatMsg" class="setting-feedback" role="status">{{ wechatMsg }}</p>
                </template>
              </div>
            </section>
          </template>
        </template>

        <template v-else-if="active === 'music'">
          <section class="setting-group">
            <SettingSwitch v-model="st.diagnostics.musicPoll" :label="t('diagMusicPoll')" />
          </section>
          <section class="setting-section">
            <h2 class="setting-group-title">{{ t('settingsMusicTitle') }}</h2>
            <div class="setting-group">
              <SettingSwitch
                v-model="st.music.neteaseBridge"
                :label="t('musicNeteaseLabel')"
                :description="t('settingsMusicHint')"
              >
                <span
                  v-if="st.music.neteaseBridge"
                  class="setting-status"
                  :title="bridgeStatusText"
                  role="status"
                  >{{ bridgeStatusText }}</span
                >
              </SettingSwitch>
            </div>
          </section>
        </template>

        <template v-else-if="active === 'diag'">
          <section class="setting-group">
            <div class="setting-row">
              <div class="setting-label">{{ t('diagLogLabel') }}</div>
              <button class="setting-action-btn" @click="systemApi.revealDataDir()">
                {{ t('diagLogReveal') }}
              </button>
            </div>
            <SettingSwitch v-model="st.diagnostics.devOverlay" :label="t('diagDevOverlay')" />
          </section>
          <section class="setting-section">
            <h2 class="setting-group-title">{{ t('settingsDataTitle') }}</h2>
            <div class="setting-group">
              <div class="setting-row">
                <div class="setting-label">{{ t('settingsResetLabel') }}</div>
                <button class="setting-action-btn danger" :disabled="resetArmed" @click="armReset">
                  {{ t('settingsResetBtn') }}
                </button>
              </div>
              <div
                v-if="resetArmed"
                class="reset-prompt"
                role="group"
                :aria-label="t('settingsResetTitle')"
                @keydown.esc.stop="resetArmed = false"
              >
                <p>{{ t('settingsResetHint') }}</p>
                <p v-if="resetError" class="setting-error" role="alert">{{ resetError }}</p>
                <div class="reset-confirm">
                  <button
                    ref="resetCancelEl"
                    class="setting-action-btn"
                    :disabled="resetBusy"
                    @click="resetArmed = false"
                  >
                    {{ t('settingsResetCancel') }}
                  </button>
                  <button class="setting-action-btn danger" :disabled="resetBusy" @click="confirmReset">
                    {{ t('settingsResetConfirm') }}
                  </button>
                </div>
              </div>
            </div>
          </section>
        </template>

        <template v-else-if="active === 'about'">
          <div class="settings-app-identity">
            <img class="settings-app-icon" :src="appIcon" alt="" />
            <div>
              <h2>Top Island</h2>
              <p>{{ appVersionLabel }}</p>
            </div>
          </div>
          <section class="setting-group">
            <div class="setting-row">
              <div class="setting-label" :title="updateMsg" role="status">
                {{ updateMsg || t('settingsUpdateTitle') }}
              </div>
              <button class="setting-action-btn" :disabled="updateBusy" @click="checkForUpdate">
                <i v-if="updateBusy" class="fa-solid fa-spinner fa-spin" aria-hidden="true"></i>
                {{ updateCanInstall ? t('updateRestart') : t('settingsCheckUpdate') }}
              </button>
            </div>
          </section>
        </template>
      </main>
    </div>
  </div>
</template>
