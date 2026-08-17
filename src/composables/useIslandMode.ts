import { ref, watchEffect } from 'vue';
import { api } from '../api';

export type IslandMode = 'still' | 'quick' | 'large';

interface IslandModeOptions {
  /** 窗口是否捕获鼠标（不穿透） */
  keepInteractive: () => boolean;
  /** 悬停时是否维持当前内容形态（不切 quick） */
  holdMode?: () => boolean;
  /** 岛主体元素包围盒；悬停看门狗据此兜底校验光标是否真的还在岛上 */
  getRect?: () => DOMRect | null;
}

/**
 * 岛形态状态机：still（胶囊）/ quick（悬停展开）/ large（面板视图）。
 * 鼠标穿透不散落在各处调用，而是由状态统一派生（watchEffect）。
 */
/** 隐藏态下悬停多久后自动唤出（ms)；点击细边仍可立即唤出 */
const REVEAL_HOVER_DELAY = 2500;
/**
 * 悬停看门狗周期（ms）。鼠标快速划过时 mouseleave 可能不触发
 * （如快速甩出屏幕/切到别的显示器），悬停态会卡死；
 * 期间用全局光标位置周期兜底校验，不依赖事件。
 */
const HOVER_WATCHDOG_MS = 500;
/** 看门狗判定的岛外余量（px），避免贴边抖动误判 */
const HOVER_MARGIN = 8;

export function useIslandMode(options: IslandModeOptions) {
  const mode = ref<IslandMode>('still');
  const isHovered = ref(false);
  /** 上滑隐藏态：岛缩到屏幕顶缘只留一道细边，悬停片刻唤出 */
  const isHidden = ref(false);

  let revealTimer: number | null = null;
  let watchdogTimer: number | null = null;

  const holdMode = options.holdMode ?? options.keepInteractive;

  watchEffect(() => {
    const interactive = mode.value !== 'still' || isHovered.value || options.keepInteractive();
    api.setIgnoreMouseEvents(!interactive).catch(() => {});
  });

  async function watchdogCheck() {
    if (!isHovered.value || mode.value === 'large') return;
    const rect = options.getRect?.();
    if (!rect) return;
    try {
      const p = await api.getCursorPoint();
      const outside =
        p.x < rect.left - HOVER_MARGIN ||
        p.x > rect.right + HOVER_MARGIN ||
        p.y < rect.top - HOVER_MARGIN ||
        p.y > rect.bottom + HOVER_MARGIN;
      if (outside) onLeave();
    } catch {}
  }

  // 悬停期间才运行看门狗（large 模式的关闭由点击遮罩等显式交互负责）
  watchEffect(() => {
    const need = isHovered.value && mode.value !== 'large';
    if (need && watchdogTimer === null) {
      watchdogTimer = window.setInterval(watchdogCheck, HOVER_WATCHDOG_MS);
    } else if (!need && watchdogTimer !== null) {
      clearInterval(watchdogTimer);
      watchdogTimer = null;
    }
  });

  function onEnter() {
    isHovered.value = true;
    if (isHidden.value) {
      // 悬停片刻再唤出，避免鼠标扫过顶部误触
      if (revealTimer === null) {
        revealTimer = window.setTimeout(() => {
          revealTimer = null;
          isHidden.value = false;
        }, REVEAL_HOVER_DELAY);
      }
      return;
    }
    if (mode.value === 'large' || holdMode()) return;
    mode.value = 'quick';
  }

  function onLeave() {
    isHovered.value = false;
    if (revealTimer !== null) {
      clearTimeout(revealTimer);
      revealTimer = null;
    }
    if (mode.value === 'large' || holdMode()) return;
    mode.value = 'still';
  }

  /** 展开到大视图；已在大视图则返回 false */
  function expand(): boolean {
    if (isHidden.value || mode.value === 'large') return false;
    mode.value = 'large';
    return true;
  }

  function collapse() {
    mode.value = 'still';
  }

  function hide() {
    if (mode.value === 'large') return;
    mode.value = 'still';
    isHidden.value = true;
  }

  return { mode, isHovered, isHidden, onEnter, onLeave, expand, collapse, hide };
}
