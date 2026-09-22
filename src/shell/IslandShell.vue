<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watchEffect } from 'vue';
import { updateApi } from '@/platform/update';
import { startClock, currentTime } from '@/core/clock';
import { useI18n } from '@/core/i18n';
import { initSettings } from '@/core/settings';
import { modules, panelIndex, panels } from '@/modules/registry';
import AlertBar from '@/ui/AlertBar.vue';
import DevOverlay from '@/ui/DevOverlay.vue';
import { alertState, showAlert } from '@/ui/alert';
import PanelDeck from './PanelDeck.vue';
import StatusBar from './StatusBar.vue';
import { setShellCommands } from './commands';
import { useHideGesture } from './useHideGesture';
import { useHotRectReporter } from './useHotRegion';
import { useIslandMode } from './useIslandMode';
import { usePanelDeck } from './usePanelDeck';
import { setShellView } from './view';

const { t, initI18n } = useI18n();

const islandEl = ref<HTMLElement | null>(null);
const containerEl = ref<HTMLElement | null>(null);
const islandWidth = ref(170);

const island = useIslandMode({
  holdContent: () => hide.dragging.value || modules.some((m) => m.holdContent?.() ?? false),
});

const hide = useHideGesture({
  isEnabled: () => !isLarge.value && !island.isHidden.value,
  onHide: island.hide,
});

const isQuick = computed(() => island.mode.value === 'quick');
const isLarge = computed(() => island.mode.value === 'large');

const deck = usePanelDeck({
  panelCount: panels.length,
  isLarge,
  getContainerWidth: () => islandEl.value?.offsetWidth ?? 420,
});

const overlays = modules.filter((m) => m.overlay);

/** 提示条来了就占满胶囊，模块一律让位 */
const capsuleOwner = computed(() => {
  if (alertState.active) return null;
  let best: (typeof modules)[number] | null = null;
  for (const m of modules) {
    if (!m.capsule?.active()) continue;
    if (!best || m.capsule.priority > best.capsule!.priority) best = m;
  }
  return best;
});

/** 没有模块占胶囊时露出壳自己的内容：still 显示时间，quick 显示状态栏 */
const showOwnContent = computed(() => !alertState.active && capsuleOwner.value === null);

const islandStyle = computed(() => {
  const style: Record<string, string> = {};
  const width = capsuleOwner.value?.capsule?.width?.();
  if (width) style.width = `${width}px`;
  if (hide.dragging.value && hide.offset.value < 0) {
    style.transform = `translateX(-50%) translateY(${hide.offset.value}px)`;
    style.transition = 'none';
  }
  return style;
});

watchEffect(() => {
  setShellView({
    mode: island.mode.value,
    hidden: island.isHidden.value,
    capsuleOwner: capsuleOwner.value?.id ?? null,
    islandWidth: islandWidth.value,
    dragOffset: hide.dragging.value ? hide.offset.value : 0,
  });
});

const hot = useHotRectReporter({
  deps: () => {
    void island.mode.value;
    void island.isHidden.value;
    void capsuleOwner.value;
    void islandWidth.value;
  },
  keepInteractive: () => hide.dragging.value || modules.some((m) => m.keepInteractive?.() ?? false),
  isFullView: () => isLarge.value,
  getIslandRect: () => islandEl.value?.getBoundingClientRect() ?? null,
  // 大视图才报整个容器；平时绝不能报它——它是全屏容器，报出去热区就是整窗，穿透全废
  getFullRect: () => containerEl.value?.getBoundingClientRect() ?? null,
});

setShellCommands({
  reveal: island.reveal,
  openPanel: (id) => {
    island.reveal();
    island.expand();
    const idx = panelIndex(id);
    if (idx >= 0) deck.switchPanel(idx);
  },
});

/** 展开后跳到最该看的那一页；谁都不要求就停在原页 */
function jumpToRequestedPanel() {
  for (const m of modules) {
    const target = m.expandTarget?.();
    if (!target) continue;
    const idx = panelIndex(target);
    if (idx >= 0) deck.switchPanel(idx);
    return;
  }
}

function onPointerDown(e: PointerEvent) {
  deck.onPointerDown(e, islandEl.value);
  hide.onPointerDown(e, islandEl.value);
}

function onPointerMove(e: PointerEvent) {
  deck.onPointerMove(e);
  hide.onPointerMove(e);
}

function onPointerUp(e: PointerEvent) {
  deck.onPointerUp(e);
  hide.end(e, islandEl.value, true);
}

function onIslandLeave() {
  // 拖动中指针离开元素属于正常路径，不触发形态收起
  if (hide.dragging.value) return;
  island.onLeave();
}

function onIslandClick(e: MouseEvent) {
  if (hide.consumeSuppressedClick()) return;
  if (island.isHidden.value) {
    // 点顶部细边立即唤出
    island.reveal();
    return;
  }
  if ((e.target as HTMLElement).closest('button, input, select')) return;
  if (deck.consumeSuppressedClick()) return;
  if (island.expand()) jumpToRequestedPanel();
}

function onContainerLeave() {
  if (!isLarge.value && island.isHovered.value) island.onLeave();
}

function onContainerMouseDown(e: MouseEvent) {
  if (isLarge.value && e.target === e.currentTarget) island.collapse();
}

function onDocMouseDown(e: MouseEvent) {
  if (isLarge.value && !islandEl.value?.contains(e.target as Node)) island.collapse();
}

/** 岛窗是顶部小窗，点游戏/其他应用不经过 shield，靠失焦兜底收起 */
function onWindowBlur() {
  if (isLarge.value) island.collapse();
}

function onFocusOut() {
  setTimeout(() => {
    const tag = document.activeElement?.tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
    if (!island.isHovered.value) island.collapse();
  }, 100);
}

let resizeObserver: ResizeObserver | null = null;

onMounted(async () => {
  await initI18n();
  await initSettings();
  await Promise.all(modules.map((m) => m.setup?.()));
  startClock();

  document.addEventListener('mousedown', onDocMouseDown);
  window.addEventListener('focusout', onFocusOut);
  window.addEventListener('blur', onWindowBlur);

  updateApi.onDownloaded((info) => {
    showAlert({
      icon: 'fa-arrow-up',
      text: t('updateReady', info.version),
      duration: 0,
      dismissible: true,
      actionLabel: t('updateRestart'),
      actionHandler: () => void updateApi.install(),
    });
  });

  if (islandEl.value) {
    resizeObserver = new ResizeObserver(() => {
      const w = islandEl.value?.offsetWidth;
      if (w) islandWidth.value = w;
      hot.report();
    });
    resizeObserver.observe(islandEl.value);
    if (containerEl.value) resizeObserver.observe(containerEl.value);
  }
  deck.warmup();
});

onBeforeUnmount(() => {
  document.removeEventListener('mousedown', onDocMouseDown);
  window.removeEventListener('focusout', onFocusOut);
  window.removeEventListener('blur', onWindowBlur);
  resizeObserver?.disconnect();
  resizeObserver = null;
});
</script>

<template>
  <div v-if="isLarge" class="large-dismiss-shield" @mousedown="island.collapse()"></div>
  <div
    id="island-container"
    ref="containerEl"
    @mouseleave="onContainerLeave"
    @mousedown="onContainerMouseDown"
  >
    <div
      id="island"
      ref="islandEl"
      :class="[
        capsuleOwner ? `capsule-${capsuleOwner.id}` : '',
        {
          quick: isQuick || alertState.active,
          large: isLarge,
          'has-alert': alertState.active,
          hidden: island.isHidden.value,
        },
      ]"
      :style="islandStyle"
      @mouseenter="island.onEnter()"
      @mouseleave="onIslandLeave"
      @click="onIslandClick"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="hide.end($event, islandEl, false)"
      @wheel.passive="deck.onWheel"
    >
      <AlertBar v-if="alertState.active" />

      <template v-if="!isLarge">
        <component :is="capsuleOwner.capsule!.component" v-if="capsuleOwner" />
        <div v-else-if="showOwnContent && !isQuick" class="still-content">
          <span class="still-time">{{ currentTime || '--:--' }}</span>
        </div>
        <StatusBar v-else-if="showOwnContent" />
      </template>

      <PanelDeck :deck="deck" :visible="isLarge" />
    </div>

    <component :is="m.overlay" v-for="m in overlays" :key="m.id" />
    <DevOverlay />
  </div>
</template>
