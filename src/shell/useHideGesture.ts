import { ref } from 'vue';

/** 命中这些控件时不启动隐藏手势，让控件自己处理 */
const NON_GESTURE_SELECTOR = 'button, input, select, textarea, .alert-content';

/** 跟手位移上限（px），再往上拖也不动 */
const MAX_OFFSET = 44;
/** 触发隐藏的最小上滑距离（px） */
const TRIGGER_DISTANCE = 24;

/**
 * 在岛上向上拖动把它收进屏幕顶缘。跟手位移期间关掉过渡，
 * 松手后由 shell 决定收起还是弹回。
 */
export function useHideGesture(options: { isEnabled: () => boolean; onHide: () => void }) {
  const dragging = ref(false);
  const offset = ref(0);

  let startX = 0;
  let startY = 0;
  let justHidden = false;

  function onPointerDown(e: PointerEvent, captureEl: HTMLElement | null) {
    if ((e.target as HTMLElement).closest(NON_GESTURE_SELECTOR)) return;
    if (!options.isEnabled()) return;
    startX = e.clientX;
    startY = e.clientY;
    dragging.value = true;
    offset.value = 0;
    captureEl?.setPointerCapture?.(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging.value) return;
    offset.value = Math.max(-MAX_OFFSET, Math.min(0, e.clientY - startY));
  }

  function end(e: PointerEvent, captureEl: HTMLElement | null, apply: boolean) {
    if (!dragging.value) return;
    dragging.value = false;
    offset.value = 0;
    captureEl?.releasePointerCapture?.(e.pointerId);
    if (!apply) return;
    const dy = e.clientY - startY;
    if (dy < -TRIGGER_DISTANCE && Math.abs(dy) > Math.abs(e.clientX - startX)) {
      options.onHide();
      justHidden = true;
    }
  }

  /** 消费一次「刚隐藏」标记；返回 true 表示本次 click 应被忽略 */
  function consumeSuppressedClick(): boolean {
    if (!justHidden) return false;
    justHidden = false;
    return true;
  }

  return { dragging, offset, onPointerDown, onPointerMove, end, consumeSuppressedClick };
}
