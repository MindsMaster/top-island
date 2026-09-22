import { ref } from 'vue';
import { windowApi } from '@/platform/window';
import type { IslandMode } from './view';

/** 隐藏态下悬停多久后自动唤出（ms）；点击细边仍可立即唤出 */
const REVEAL_HOVER_DELAY = 2500;

interface IslandModeOptions {
  /** 悬停时是否维持当前内容形态（不切 quick） */
  holdContent: () => boolean;
}

/**
 * 岛形态状态机：still（胶囊）/ quick（悬停展开）/ large（面板视图），外加上滑隐藏态。
 * 悬停以 Rust 钩子的 island-hover 事件为准（穿透态下 DOM 事件拿不到），
 * DOM mouseenter/mouseleave 只作可交互态下的低延迟冗余，两边调用都幂等。
 */
export function useIslandMode(options: IslandModeOptions) {
  const mode = ref<IslandMode>('still');
  const isHovered = ref(false);
  const isHidden = ref(false);

  let revealTimer: number | null = null;

  function clearRevealTimer() {
    if (revealTimer === null) return;
    clearTimeout(revealTimer);
    revealTimer = null;
  }

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
    if (mode.value === 'large' || options.holdContent()) return;
    mode.value = 'quick';
  }

  function onLeave() {
    isHovered.value = false;
    clearRevealTimer();
    if (mode.value === 'large' || options.holdContent()) return;
    mode.value = 'still';
  }

  /** 展开到大视图；已在大视图或隐藏态则返回 false */
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

  function reveal() {
    clearRevealTimer();
    isHidden.value = false;
  }

  windowApi.onHover((inside) => (inside ? onEnter() : onLeave()));

  return { mode, isHovered, isHidden, onEnter, onLeave, expand, collapse, hide, reveal };
}
