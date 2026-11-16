import { onUnmounted } from 'vue';

/** 组件自管拖动或滚轮 外壳手势不介入 */
export const NO_GESTURE = '[data-no-gesture]';

/** 命中则不起手势也不点击展开 */
export const CONTROLS = `button, input, select, textarea, ${NO_GESTURE}`;

const CAPTURE_SLOP = 5;

export interface DragHandler {
  begin(): void;
  move(dx: number, dy: number): void;
  end(dx: number, dy: number, cancelled: boolean): void;
}

export function usePointerDrag(pick: () => DragHandler | null) {
  let handler: DragHandler | null = null;
  let el: HTMLElement | null = null;
  let pointerId = -1;
  let startX = 0;
  let startY = 0;
  let captured = false;
  let swallowClick = false;

  /** 捕获前指针可能已离岛 收尾不能只靠岛上事件 */
  function listen(on: boolean) {
    if (on) {
      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', onUp);
      window.addEventListener('pointercancel', onCancel);
    } else {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      window.removeEventListener('pointercancel', onCancel);
    }
  }

  function finish(e: PointerEvent, cancelled: boolean) {
    if (!handler) return;
    listen(false);
    const h = handler;
    handler = null;
    swallowClick = captured && !cancelled;
    h.end(e.clientX - startX, e.clientY - startY, cancelled);
  }

  function onPointerDown(e: PointerEvent) {
    if (handler && e.pointerId !== pointerId) return;
    finish(e, true);
    swallowClick = false;
    if (e.button !== 0 || (e.target as Element).closest(CONTROLS)) return;
    handler = pick();
    if (!handler) return;
    el = e.currentTarget as HTMLElement;
    pointerId = e.pointerId;
    startX = e.clientX;
    startY = e.clientY;
    captured = false;
    listen(true);
    handler.begin();
  }

  function onMove(e: PointerEvent) {
    if (!handler || e.pointerId !== pointerId) return;
    if ((e.buttons & 1) === 0) {
      finish(e, true);
      return;
    }
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    // 按下即捕获会吞掉子元素点击
    if (!captured && Math.hypot(dx, dy) > CAPTURE_SLOP) {
      el?.setPointerCapture(e.pointerId);
      captured = true;
    }
    handler.move(dx, dy);
  }

  function onUp(e: PointerEvent) {
    if (e.pointerId === pointerId) finish(e, false);
  }

  function onCancel(e: PointerEvent) {
    if (e.pointerId === pointerId) finish(e, true);
  }

  function consumeClick(): boolean {
    const swallow = swallowClick;
    swallowClick = false;
    return swallow;
  }

  onUnmounted(() => listen(false));

  return { onPointerDown, consumeClick };
}
