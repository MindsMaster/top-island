import { ref } from 'vue';
import { windowApi } from '@/platform/window';
import type { IslandMode } from './view';

const REVEAL_HOVER_DELAY = 2500;

interface IslandModeOptions {
  /** 悬停不切 quick */
  holdContent: () => boolean;
}

/** 穿透态 DOM 事件拿不到 悬停以 island:hover 为准 */
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
      // 延迟唤出防扫过误触
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
