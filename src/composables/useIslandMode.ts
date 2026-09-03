import { ref, watchEffect } from 'vue';
import { api } from '../api';

export type IslandMode = 'still' | 'quick' | 'large';

interface IslandModeOptions {
  /** 是否全程可交互（hide 拖动、通知卡片悬停等）：为真时热区上报 null，穿透关闭 */
  keepInteractive: () => boolean;
  /** 悬停时是否维持当前内容形态（不切 quick） */
  holdMode?: () => boolean;
  /** 岛主体（含通知栈）的包围盒，作为上报给 Rust 钩子的交互热区 */
  getRect?: () => DOMRect | null;
}

/**
 * 岛形态状态机：still（胶囊）/ quick（悬停展开）/ large（面板视图）。
 * 悬停判定以 Rust 钩子的 island-hover 事件为准（穿透态下 DOM 事件拿不到），
 * DOM mouseenter/mouseleave 只作可交互态下的低延迟冗余，两边调用都幂等。
 * 穿透不再由前端逐点控制：热区经 setHotRect 上报后由 Rust 钩子统一切换。
 */
/** 隐藏态下悬停多久后自动唤出（ms)；点击细边仍可立即唤出 */
const REVEAL_HOVER_DELAY = 2500;

export function useIslandMode(options: IslandModeOptions) {
  const mode = ref<IslandMode>('still');
  const isHovered = ref(false);
  /** 上滑隐藏态：岛缩到屏幕顶缘只留一道细边，悬停片刻唤出 */
  const isHidden = ref(false);

  let revealTimer: number | null = null;

  const holdMode = options.holdMode ?? options.keepInteractive;

  // 热区上报：keepInteractive 为真时报 null（全程可交互），否则报岛主体包围盒。
  // getRect 不是响应式的，尺寸/位置变化由调用方（ResizeObserver）触发 reportRect。
  function reportRect() {
    if (options.keepInteractive()) {
      void api.setHotRect(null).catch(() => {});
      return;
    }
    const rect = options.getRect?.();
    void api.setHotRect(rect ?? null).catch(() => {});
  }

  watchEffect(() => {
    // 依赖 mode/isHidden/keepInteractive 的变化重报热区
    void mode.value;
    void isHidden.value;
    reportRect();
  });

  api.onIslandHover((inside) => {
    if (inside) onEnter();
    else onLeave();
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

  return { mode, isHovered, isHidden, onEnter, onLeave, expand, collapse, hide, reportRect };
}
