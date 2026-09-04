<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { api } from './api';
import { useI18n } from './i18n';
import { useClock } from './composables/useClock';
import { THEMES, initSettings, settings, toggleTheme } from './store/settings';
import { alertState, dismissAlert, showAlert } from './store/alert';
import { currentLyric, marqueeText, musicState, startMusicPoll, stopMusicPoll } from './store/music';
import { completeReminderTask, initTasks, startReminderTimer, tasksState } from './store/tasks';
import {
  alarmState,
  cancelCountdown,
  displayRemainOf,
  initAlarm,
  keepInteractiveOf,
  progressOf,
} from './store/alarm';
import { initClipboard, readCurrent, startClipboardWatch } from './store/clipboard';
import {
  MAX_VISIBLE,
  activatePopup,
  closePopup,
  formatTime,
  initNotifications,
  notifyState,
  setPopupHover,
} from './store/notifications';
import { initWeather, weatherIcon, weatherState } from './store/weather';
import { useIslandMode } from './composables/useIslandMode';
import { usePanelSwipe } from './composables/usePanelSwipe';
import { panels, CALENDAR_PANEL_INDEX, ALARM_PANEL_INDEX } from './panels';
import MusicMarquee from './components/MusicMarquee.vue';

const { t, initI18n } = useI18n();
const { currentTime, currentDate, isNightTime, startClock } = useClock();
const settingsSnap = settings;
const themeIcon = computed(() => THEMES.find((tm) => tm.id === settingsSnap.theme)?.icon ?? 'fa-moon');
const musicSnap = musicState;
const tasksSnap = tasksState;
const alarmSnap = alarmState;
const notifySnap = notifyState;
const weatherSnap = weatherState;
const alertSnap = alertState;

const lyric = computed(() => currentLyric(musicSnap));
const marquee = computed(() => marqueeText(musicSnap));
const alarmKeepInteractive = computed(() => keepInteractiveOf(alarmSnap));
const alarmProgress = computed(() => progressOf(alarmSnap));
const alarmDisplayRemain = computed(() => displayRemainOf(alarmSnap));
const visiblePopups = computed(() => notifySnap.popups.slice(0, MAX_VISIBLE));
const foldedCount = computed(() => Math.max(0, notifySnap.popups.length - MAX_VISIBLE));
const weatherIconCls = computed(() => weatherIcon(weatherSnap, isNightTime.value));

/** 点击提示条的动作按钮：执行后立即消费收起（固定时长只是不点时的兜底） */
function onAlertAction(handler: (() => void) | null) {
  handler?.();
  dismissAlert();
}

const islandEl = ref<HTMLElement | null>(null);
const containerEl = ref<HTMLElement | null>(null);

const hideDragging = ref(false);
const hideDragOffset = ref(0);
const miniHover = ref(false);

/** setup 完成后才置真：useIslandMode 的 watchEffect 首次同步运行时 alarmMini 等还没声明（TDZ），
 *  那几轮上报只给岛本体，mounted 里置真后会重报完整热区 */
let rectsArmed = false;

const island = useIslandMode({
  keepInteractive: () => hideDragging.value || notifySnap.hoveredPopup !== null || miniHover.value,
  holdMode: () => tasksState.activeReminderTask !== null || alarmKeepInteractive.value || hideDragging.value,
  // 热区 = 岛本体，叠上可见的通知栈/迷你闹钟（和岛不重叠，取包围盒即可）；
  // 大视图整窗可交互（点面板外的 shield 要收起）。
  // 绝不能报 container——它是全屏容器，报出去热区就是整窗，穿透全废
  getRect: () => {
    const islandRect = islandEl.value?.getBoundingClientRect();
    if (!islandRect || !rectsArmed) return islandRect ?? null;
    if (island.mode.value === 'large') {
      return containerEl.value?.getBoundingClientRect() ?? islandRect;
    }
    let left = islandRect.left;
    let top = islandRect.top;
    let right = islandRect.right;
    let bottom = islandRect.bottom;
    const include = (el: Element | null | undefined) => {
      const r = el?.getBoundingClientRect();
      if (!r || r.width === 0 || r.height === 0) return;
      left = Math.min(left, r.left);
      top = Math.min(top, r.top);
      right = Math.max(right, r.right);
      bottom = Math.max(bottom, r.bottom);
    };
    const root = containerEl.value;
    // 隐藏态通知栈 dock-hidden 不可见，不并进热区
    if (visiblePopups.value.length && !island.isHidden.value) {
      include(root?.querySelector('.notify-stack'));
    }
    if (alarmMini.value) include(root?.querySelector('.alarm-mini'));
    return new DOMRect(left, top, right - left, bottom - top);
  },
});

const swipe = usePanelSwipe({
  panelCount: panels.length,
  isEnabled: () => island.mode.value === 'large',
  getContainerWidth: () => islandEl.value?.offsetWidth ?? 420,
});

const isQuickView = computed(() => island.mode.value === 'quick');
const isLargeView = computed(() => island.mode.value === 'large');

const LYRIC_MIN_WIDTH = 190;
const LYRIC_MAX_WIDTH = 800;
/** 歌词文本以外的固定占位：岛 padding + 封面 + 间距 */
const LYRIC_FIXED_WIDTH = 72;

let measureCtx: CanvasRenderingContext2D | null = null;

function measureLyricWidth(text: string): number {
  if (!measureCtx) measureCtx = document.createElement('canvas').getContext('2d');
  if (!measureCtx) return 0;
  measureCtx.font = "500 13px 'OpenRunde', -apple-system, 'Segoe UI', Roboto, sans-serif";
  return measureCtx.measureText(text).width;
}

const islandStyle = computed(() => {
  const style: Record<string, string> = {};
  if (island.mode.value === 'still' && showMusicQuick.value && lyric.value) {
    const w = Math.round(
      Math.min(LYRIC_MAX_WIDTH, Math.max(LYRIC_MIN_WIDTH, measureLyricWidth(lyric.value) + LYRIC_FIXED_WIDTH))
    );
    style.width = w + 'px';
  }
  // 上滑手势跟手位移（拖动期间关闭过渡保证跟手）
  if (hideDragging.value && hideDragOffset.value < 0) {
    style.transform = `translateX(-50%) translateY(${hideDragOffset.value}px)`;
    style.transition = 'none';
  }
  return style;
});
const showMusicQuick = computed(
  () => musicSnap.hasMusic && musicSnap.isPlaying && island.mode.value === 'still'
);
const showReminderInQuick = computed(
  () => tasksSnap.activeReminderTask !== null && island.mode.value !== 'large'
);
const showAlarmInQuick = computed(() => alarmSnap.countdown.running && island.mode.value !== 'large');
const capsuleBusy = computed(() => showMusicQuick.value || showReminderInQuick.value);
const alarmInCapsule = computed(() => showAlarmInQuick.value && !capsuleBusy.value);
const alarmMini = computed(
  () =>
    alarmSnap.countdown.running &&
    island.mode.value !== 'large' &&
    (capsuleBusy.value || island.isHidden.value)
);
watch(alarmMini, (mini) => {
  if (!mini) miniHover.value = false;
});

/** 已挂载的面板。首次进大视图只挂当前面板和相邻，其余在动画期间的空闲帧补齐——
 *  一次挂 7 个面板会产生 60~85ms 的长任务，摊开后展开不卡。
 *  面板挂载后不再卸载（外层改 v-show），之后展开/收起零挂载成本 */
const mountedPanels = ref<Set<number>>(new Set());
let backfillTimer: number | null = null;

function ensurePanel(i: number) {
  if (i >= 0 && i < panels.length) mountedPanels.value.add(i);
}

watch(isLargeView, (large) => {
  if (!large) return;
  ensurePanel(swipe.activePanel.value);
  ensurePanel(swipe.activePanel.value - 1);
  ensurePanel(swipe.activePanel.value + 1);
  if (backfillTimer !== null) clearInterval(backfillTimer);
  let next = 0;
  backfillTimer = window.setInterval(() => {
    while (next < panels.length && mountedPanels.value.has(next)) next++;
    if (next >= panels.length) {
      if (backfillTimer !== null) clearInterval(backfillTimer);
      backfillTimer = null;
      return;
    }
    mountedPanels.value.add(next);
  }, 60);
});

// 滑到未挂载的面板时立即补上（补齐通常早已完成，这是兜底）
watch(swipe.activePanel, (i) => {
  if (!isLargeView.value) return;
  ensurePanel(i);
  ensurePanel(i - 1);
  ensurePanel(i + 1);
});

// 通知卡片进出/迷你闹钟显隐都会改热区包围盒，重报；卡片有 0.28s 进出场动画，补一次延迟重报
watch([() => visiblePopups.value.length, alarmMini], () => {
  island.reportRect();
  window.setTimeout(() => island.reportRect(), 300);
});

function onMiniClick() {
  if (island.isHidden.value) {
    island.isHidden.value = false;
    return;
  }
  if (island.expand()) swipe.switchPanel(ALARM_PANEL_INDEX);
}

const islandWidth = ref(170);
let islandRO: ResizeObserver | null = null;
const MINI_GAP = 10;
const alarmMiniStyle = computed(() => {
  const style: Record<string, string> = {
    left: `calc(50% + ${Math.round(islandWidth.value / 2) + MINI_GAP}px)`,
  };
  if (hideDragging.value && hideDragOffset.value < 0) {
    style.translate = `0 ${hideDragOffset.value}px`;
    style.transition = 'none';
  } else if (island.isHidden.value) {
    style.translate = `0 var(--island-hidden-shift, -34px)`;
  }
  return style;
});

let hideStartX = 0;
let hideStartY = 0;
let suppressClickAfterHide = false;

function onIslandPointerDown(e: PointerEvent) {
  swipe.onPointerDown(e, islandEl.value);
  if ((e.target as HTMLElement).closest('button, input, select, textarea, .alert-content')) return;
  if (island.mode.value !== 'large' && !island.isHidden.value) {
    hideStartX = e.clientX;
    hideStartY = e.clientY;
    hideDragging.value = true;
    hideDragOffset.value = 0;
    islandEl.value?.setPointerCapture?.(e.pointerId);
  }
}

function onIslandPointerMove(e: PointerEvent) {
  swipe.onPointerMove(e);
  if (hideDragging.value) {
    hideDragOffset.value = Math.max(-44, Math.min(0, e.clientY - hideStartY));
  }
}

function endHideGesture(e: PointerEvent, apply: boolean) {
  if (!hideDragging.value) return;
  hideDragging.value = false;
  hideDragOffset.value = 0;
  islandEl.value?.releasePointerCapture?.(e.pointerId);
  if (!apply) return;
  const dx = e.clientX - hideStartX;
  const dy = e.clientY - hideStartY;
  if (dy < -24 && Math.abs(dy) > Math.abs(dx)) {
    island.hide();
    suppressClickAfterHide = true;
  }
}

function onIslandPointerUp(e: PointerEvent) {
  swipe.onPointerUp(e);
  endHideGesture(e, true);
}

function onIslandPointerCancel(e: PointerEvent) {
  endHideGesture(e, false);
}

function onIslandMouseLeave() {
  // 拖动中指针离开元素属于正常路径，不触发形态收起
  if (hideDragging.value) return;
  island.onLeave();
}

function onIslandClick(e: MouseEvent) {
  if (suppressClickAfterHide) {
    suppressClickAfterHide = false;
    return;
  }
  if (island.isHidden.value) {
    // 点击顶部细边立即唤出
    island.isHidden.value = false;
    return;
  }
  const target = e.target as HTMLElement;
  if (target.closest('button') || target.closest('input') || target.closest('select')) return;
  if (swipe.consumeSuppressedClick()) return;
  if (island.expand()) {
    if (tasksState.activeReminderTask) {
      swipe.switchPanel(CALENDAR_PANEL_INDEX);
    } else if (alarmState.countdown.running) {
      swipe.switchPanel(ALARM_PANEL_INDEX);
    }
  }
}

function onDocMouseDown(e: MouseEvent) {
  if (island.mode.value === 'large' && !islandEl.value?.contains(e.target as Node)) {
    island.collapse();
  }
}

function onWindowBlur() {
  // 岛窗口已是顶部小窗：点击窗外（游戏/其他应用）不再经过 shield，
  // 由窗口失焦兜底收起大视图
  if (island.mode.value === 'large') island.collapse();
}

function onContainerLeave() {
  if (island.mode.value !== 'large' && island.isHovered.value) island.onLeave();
}

function onContainerMouseDown(e: MouseEvent) {
  if (island.mode.value === 'large' && e.target === e.currentTarget) island.collapse();
}

function onFocusOut() {
  setTimeout(() => {
    const tag = document.activeElement?.tagName;
    if (tag !== 'INPUT' && tag !== 'TEXTAREA' && tag !== 'SELECT' && !island.isHovered.value) {
      island.collapse();
    }
  }, 100);
}

onMounted(async () => {
  await initI18n();
  await initSettings();
  await Promise.all([initTasks(), initAlarm(), initClipboard(), initNotifications()]);

  startClock();
  startMusicPoll();
  void initWeather();
  startReminderTimer();
  startClipboardWatch();
  readCurrent();

  document.addEventListener('mousedown', onDocMouseDown);
  window.addEventListener('focusout', onFocusOut);
  window.addEventListener('blur', onWindowBlur);

  api.onUpdateDownloaded((info) => {
    showAlert({
      icon: 'fa-arrow-up',
      text: t('updateReady', info.version),
      duration: 0,
      dismissible: true,
      actionLabel: t('updateRestart'),
      actionHandler: () => {
        void api.installUpdate();
      },
    });
  });

  if (islandEl.value) {
    islandRO = new ResizeObserver(() => {
      const w = islandEl.value?.offsetWidth;
      if (w) islandWidth.value = w;
      island.reportRect();
    });
    islandRO.observe(islandEl.value);
    if (containerEl.value) islandRO.observe(containerEl.value);
  }
  rectsArmed = true;
  island.reportRect();
});

onBeforeUnmount(() => {
  document.removeEventListener('mousedown', onDocMouseDown);
  window.removeEventListener('focusout', onFocusOut);
  window.removeEventListener('blur', onWindowBlur);
  islandRO?.disconnect();
  islandRO = null;
  stopMusicPoll();
});

function closeWindow() {
  api.closeWindow();
}
</script>

<template>
  <div v-if="isLargeView" class="large-dismiss-shield" @mousedown="island.collapse()"></div>
  <div
    id="island-container"
    ref="containerEl"
    @mouseleave="onContainerLeave"
    @mousedown="onContainerMouseDown"
  >
    <div
      id="island"
      ref="islandEl"
      :class="{
        quick: isQuickView || showMusicQuick || alertSnap.active,
        large: isLargeView,
        'has-alert': alertSnap.active,
        'show-reminder': showReminderInQuick,
        'has-alarm': alarmInCapsule,
        hidden: island.isHidden.value,
      }"
      :style="islandStyle"
      @mouseenter="island.onEnter()"
      @mouseleave="onIslandMouseLeave"
      @click="onIslandClick"
      @pointerdown="onIslandPointerDown"
      @pointermove="onIslandPointerMove"
      @pointerup="onIslandPointerUp"
      @pointercancel="onIslandPointerCancel"
      @wheel.passive="swipe.onWheel"
    >
      <div
        v-if="alertSnap.active"
        class="alert-content"
        @click.stop="alertSnap.actionHandler ? null : dismissAlert()"
      >
        <i :class="'fa-solid ' + alertSnap.icon"></i>
        <span class="alert-text">{{ alertSnap.text }}</span>
        <button
          v-if="alertSnap.secondLabel"
          class="alert-action-btn secondary"
          @click.stop="onAlertAction(alertSnap.secondHandler)"
        >
          {{ alertSnap.secondLabel }}
        </button>
        <button
          v-if="alertSnap.actionLabel"
          class="alert-action-btn"
          @click.stop="onAlertAction(alertSnap.actionHandler)"
        >
          {{ alertSnap.actionLabel }}
        </button>
        <i
          v-if="alertSnap.dismissible"
          class="fa-solid fa-xmark alert-close"
          @click.stop="dismissAlert()"
        ></i>
      </div>

      <template v-if="!isLargeView">
        <div v-if="showReminderInQuick && !alertSnap.active" class="reminder-content">
          <div class="reminder-top">
            <i class="fa-solid fa-clock reminder-icon"></i>
            <span class="reminder-text">{{ tasksSnap.activeReminderTask!.text }}</span>
            <button class="reminder-done-btn" @click.stop="completeReminderTask()">
              <i class="fa-solid fa-check"></i>
            </button>
          </div>
          <div class="reminder-hint">{{ t('reminderHint') }}</div>
        </div>

        <Transition name="capfade">
          <div v-if="alarmInCapsule && !alertSnap.active" class="alarm-quick">
            <svg class="alarm-quick-ring" viewBox="0 0 32 32" width="30" height="30">
              <circle cx="16" cy="16" r="13" fill="none" stroke="rgba(128,128,128,0.28)" stroke-width="3.5" />
              <circle
                cx="16"
                cy="16"
                r="13"
                fill="none"
                stroke="var(--accent)"
                stroke-width="3.5"
                stroke-linecap="round"
                :stroke-dasharray="2 * Math.PI * 13"
                :stroke-dashoffset="2 * Math.PI * 13 * (1 - alarmProgress)"
                transform="rotate(-90 16 16)"
              />
            </svg>
            <span class="alarm-quick-time">{{ alarmDisplayRemain }}</span>
            <button class="alarm-quick-stop" @click.stop="cancelCountdown()">
              <i class="fa-solid fa-xmark"></i>
            </button>
          </div>
        </Transition>

        <div
          v-if="
            island.mode.value === 'still' &&
            !showMusicQuick &&
            !alertSnap.active &&
            !showReminderInQuick &&
            !alarmInCapsule
          "
          class="still-content"
        >
          <span class="still-time">{{ currentTime || '--:--' }}</span>
        </div>

        <div
          v-if="
            isQuickView && !showMusicQuick && !alertSnap.active && !showReminderInQuick && !alarmInCapsule
          "
          class="quick-content"
        >
          <div v-if="weatherSnap.temp !== null" class="quick-weather">
            <i :class="'fa-solid ' + weatherIconCls"></i>
            <span>{{ weatherSnap.temp }}°</span>
            <span class="quick-weather-city">{{ weatherSnap.city }}</span>
          </div>
          <span class="quick-time">{{ currentTime }}</span>
          <span class="quick-date">{{ currentDate }}</span>
          <div class="quick-right">
            <button class="quick-settings-btn" :title="t('openSettings')" @click.stop="api.openSettings()">
              <i class="fa-solid fa-gear"></i>
            </button>
            <button class="quick-theme-btn" :title="t('themeCycle')" @click.stop="toggleTheme">
              <i :class="'fa-solid ' + themeIcon"></i>
            </button>
            <button class="quick-close-btn" :title="t('closeIsland')" @click.stop="closeWindow">
              <i class="fa-solid fa-xmark"></i>
            </button>
          </div>
        </div>

        <div
          v-if="showMusicQuick && musicSnap.hasMusic && !alertSnap.active && !showReminderInQuick"
          class="quick-content music-full"
        >
          <div class="artwork-wrap">
            <img v-if="musicSnap.artworkUrl" :src="musicSnap.artworkUrl" alt="" draggable="false" />
            <i v-else class="fa-solid fa-music"></i>
          </div>
          <div v-if="lyric" class="lyric-box">
            <span :key="lyric" class="lyric-line">{{ lyric }}</span>
          </div>
          <MusicMarquee
            v-else
            :text="marquee"
            :active="musicSnap.isPlaying && island.mode.value === 'still'"
          />
        </div>
      </template>

      <div v-show="isLargeView" class="panels-wrapper" :class="{ dragging: swipe.isDragging.value }">
        <template v-for="(p, i) in panels" :key="p.id">
          <div
            v-if="mountedPanels.has(i)"
            class="panel"
            :class="p.id + '-panel'"
            :style="swipe.panelStyle(i)"
          >
            <component :is="p.component" />
          </div>
        </template>
      </div>

      <div v-show="isLargeView" class="panel-indicator">
        <button
          v-for="(p, i) in panels"
          :key="p.id"
          class="panel-nav-btn"
          :class="{ active: swipe.activePanel.value === i }"
          :title="t(p.titleKey)"
          @click.stop="swipe.switchPanel(i)"
        >
          <i :class="'fa-solid ' + p.icon"></i>
        </button>
        <div class="panel-indicator-sep"></div>
        <button class="panel-nav-btn" :title="t('openSettings')" @click.stop="api.openSettings()">
          <i class="fa-solid fa-gear"></i>
        </button>
      </div>
    </div>

    <TransitionGroup
      name="npop"
      tag="div"
      class="notify-stack"
      :class="{ 'dock-hidden': isLargeView || island.isHidden.value }"
    >
      <div
        v-for="card in visiblePopups"
        :key="card.key"
        class="notify-card"
        @mouseenter="setPopupHover(card.key)"
        @mouseleave="setPopupHover(null)"
        @click.stop="activatePopup(card)"
      >
        <div
          class="notify-avatar"
          :class="{
            'notify-blur':
              settingsSnap.notifications.privacy.enabled && settingsSnap.notifications.privacy.blurAvatar,
          }"
        >
          <img
            v-if="notifySnap.images[card.entry.key]"
            :src="notifySnap.images[card.entry.key]"
            alt=""
            draggable="false"
          />
          <span v-else class="notify-avatar-fallback">{{ (card.entry.app || '?').slice(0, 1) }}</span>
        </div>
        <div class="notify-body">
          <div class="notify-meta">
            <span class="notify-app">{{ card.entry.app }}</span>
            <span class="notify-time">{{ formatTime(card.entry.arrival) }}</span>
          </div>
          <div
            v-if="card.entry.title"
            class="notify-title"
            :class="{
              'notify-blur':
                settingsSnap.notifications.privacy.enabled && settingsSnap.notifications.privacy.blurName,
            }"
          >
            {{ card.entry.title }}
          </div>
          <div
            v-if="
              settingsSnap.notifications.privacy.enabled && settingsSnap.notifications.privacy.replaceBody
            "
            class="notify-text notify-text-private"
          >
            {{ settingsSnap.notifications.privacy.bodyText || t('notifyPrivateBody') }}
          </div>
          <div v-else-if="card.entry.body" class="notify-text">{{ card.entry.body }}</div>
        </div>
        <button class="notify-close" @click.stop="closePopup(card.key)">
          <i class="fa-solid fa-xmark"></i>
        </button>
      </div>
      <div v-if="foldedCount > 0" key="__fold" class="notify-fold">还有 {{ foldedCount }} 条消息</div>
    </TransitionGroup>

    <Transition name="capfade">
      <div
        v-if="alarmMini"
        class="alarm-mini"
        role="button"
        :style="alarmMiniStyle"
        @mouseenter="miniHover = true"
        @mouseleave="miniHover = false"
        @click.stop="onMiniClick"
      >
        <svg class="alarm-quick-ring" viewBox="0 0 32 32" width="18" height="18">
          <circle cx="16" cy="16" r="13" fill="none" stroke="rgba(128,128,128,0.28)" stroke-width="4" />
          <circle
            cx="16"
            cy="16"
            r="13"
            fill="none"
            stroke="var(--accent)"
            stroke-width="4"
            stroke-linecap="round"
            :stroke-dasharray="2 * Math.PI * 13"
            :stroke-dashoffset="2 * Math.PI * 13 * (1 - alarmProgress)"
            transform="rotate(-90 16 16)"
          />
        </svg>
        <div class="alarm-mini-pop">
          <div class="alarm-mini-pop-card">
            <span class="alarm-mini-time">{{ alarmDisplayRemain }}</span>
            <button class="alarm-mini-stop" @click.stop="cancelCountdown()">
              <i class="fa-solid fa-xmark"></i>
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </div>
</template>
