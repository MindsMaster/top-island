<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { settingsApi } from '@/platform/settings';
import { windowApi } from '@/platform/window';
import { useI18n } from '@/core/i18n';
import { initSettings } from '@/core/settings';
import { animationsSettled } from '@/ui/animations';
import { sections } from './sections';

const { t, initI18n } = useI18n();

const active = ref(sections[0].id);
const activeSection = computed(() => sections.find((s) => s.id === active.value) ?? sections[0]);
const contentEl = ref<HTMLElement | null>(null);

watch(active, async () => {
  await nextTick();
  contentEl.value?.scrollTo(0, 0);
});

const rootEl = ref<HTMLElement | null>(null);

/** 初始即离场态 隐藏窗的残帧须透明 */
const shown = ref(false);
let leaveToken = 0;

function reveal() {
  leaveToken++;
  shown.value = true;
}

async function close() {
  if (!shown.value) return;
  shown.value = false;
  const token = ++leaveToken;
  await nextTick();
  await animationsSettled(rootEl.value?.getAnimations() ?? []);
  if (token === leaveToken) windowApi.closeSelf();
}

settingsApi.onOpened(reveal);

/** 聚焦瞬间的抖动宽限 */
const FOCUS_BLUR_GRACE_MS = 500;
let focusedAt = 0;

/** 托盘开窗不发 opened */
function onWindowFocus() {
  focusedAt = Date.now();
  reveal();
}

function onWindowBlur() {
  if (Date.now() - focusedAt < FOCUS_BLUR_GRACE_MS) return;
  close();
}

onMounted(async () => {
  await initI18n();
  await initSettings();
  document.title = t('settingsTitle');
  window.addEventListener('blur', onWindowBlur);
  window.addEventListener('focus', onWindowFocus);
});

onBeforeUnmount(() => {
  window.removeEventListener('blur', onWindowBlur);
  window.removeEventListener('focus', onWindowFocus);
});
</script>

<template>
  <div id="settings-window" ref="rootEl" :class="{ leaving: !shown }">
    <header class="settings-header">
      <span class="settings-title">{{ t('settingsTitle') }}</span>
      <button class="settings-close" :aria-label="t('settingsClose')" @click="close">
        <i class="fa-solid fa-xmark" aria-hidden="true"></i>
      </button>
    </header>

    <div class="settings-body">
      <nav class="settings-nav" :aria-label="t('settingsTitle')">
        <button
          v-for="s in sections"
          :key="s.id"
          class="settings-nav-btn"
          :class="{ active: active === s.id, 'settings-nav-bottom': s.bottom }"
          :aria-current="active === s.id ? 'page' : undefined"
          @click="active = s.id"
        >
          <i :class="'fa-solid ' + s.icon" aria-hidden="true"></i>
          <span>{{ t(s.titleKey) }}</span>
        </button>
      </nav>

      <main ref="contentEl" class="settings-content" aria-labelledby="settings-page-title">
        <h1 id="settings-page-title" class="settings-page-title">{{ t(activeSection.titleKey) }}</h1>
        <component :is="activeSection.component" :key="activeSection.id" />
      </main>
    </div>
  </div>
</template>
