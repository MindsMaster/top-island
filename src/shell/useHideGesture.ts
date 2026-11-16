import { ref } from 'vue';

/** 命中则交控件自行处理 */
const NON_GESTURE_SELECTOR = 'button, input, select, textarea, .alert-content';

const MAX_OFFSET = 44;
const TRIGGER_DISTANCE = 24;
const CAPTURE_SLOP = 5;

export function useHideGesture(options: { isEnabled: () => boolean; onHide: () => void }) {
  const dragging = ref(false);
  const offset = ref(0);

  let startX = 0;
  let startY = 0;
  let justHidden = false;
  /** 按下即捕获会吞掉子元素点击 */
  let pendingCapture: HTMLElement | null = null;

  function onPointerDown(e: PointerEvent, captureEl: HTMLElement | null) {
    if ((e.target as HTMLElement).closest(NON_GESTURE_SELECTOR)) return;
    if (!options.isEnabled()) return;
    startX = e.clientX;
    startY = e.clientY;
    dragging.value = true;
    offset.value = 0;
    pendingCapture = captureEl;
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging.value) return;
    if (pendingCapture && Math.hypot(e.clientX - startX, e.clientY - startY) > CAPTURE_SLOP) {
      pendingCapture.setPointerCapture?.(e.pointerId);
      pendingCapture = null;
    }
    offset.value = Math.max(-MAX_OFFSET, Math.min(0, e.clientY - startY));
  }

  function end(e: PointerEvent, captureEl: HTMLElement | null, apply: boolean) {
    if (!dragging.value) return;
    dragging.value = false;
    offset.value = 0;
    pendingCapture = null;
    captureEl?.releasePointerCapture?.(e.pointerId);
    if (!apply) return;
    const dy = e.clientY - startY;
    if (dy < -TRIGGER_DISTANCE && Math.abs(dy) > Math.abs(e.clientX - startX)) {
      options.onHide();
      justHidden = true;
    }
  }

  function consumeSuppressedClick(): boolean {
    if (!justHidden) return false;
    justHidden = false;
    return true;
  }

  return { dragging, offset, onPointerDown, onPointerMove, end, consumeSuppressedClick };
}
