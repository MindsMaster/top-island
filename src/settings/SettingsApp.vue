<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { settingsApi } from '@/platform/settings';
import { windowApi } from '@/platform/window';
import { useI18n } from '@/core/i18n';
import { initSettings } from '@/core/settings';
import { sections } from './sections';

const { t, initI18n } = useI18n();

const active = ref(sections[0].id);
const activeSection = computed(() => sections.find((s) => s.id === active.value) ?? sections[0]);
const contentEl = ref<HTMLElement | null>(null);

watch(active, async () => {
  await nextTick();
  contentEl.value?.scrollTo(0, 0);
});

/**
 * 每次打开窗口 +1：根节点换 key 重建以重播进入动画。
 * 窗口常驻不销毁，Vue 不会自己重挂载；重建顺带把各分区的临时状态清干净。
 */
const enterKey = ref(0);
settingsApi.onOpened(() => enterKey.value++);

/** 失焦即关。刚聚焦那一瞬的抖动不算 */
const FOCUS_BLUR_GRACE_MS = 500;
let focusedAt = 0;

function onWindowFocus() {
  focusedAt = Date.now();
}

function onWindowBlur() {
  if (Date.now() - focusedAt < FOCUS_BLUR_GRACE_MS) return;
  windowApi.closeSelf();
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
