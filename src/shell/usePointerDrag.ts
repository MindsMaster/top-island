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
  let startX = 0;
  let startY = 0;
  let captured = false;
  let swallowClick = false;

  function finish(e: PointerEvent, cancelled: boolean) {
    if (!handler) return;
    const h = handler;
    handler = null;
    swallowClick = captured && !cancelled;
    h.end(e.clientX - startX, e.clientY - startY, cancelled);
  }

  function onPointerDown(e: PointerEvent) {
    finish(e, true);
    swallowClick = false;
    if ((e.target as Element).closest(CONTROLS)) return;
    handler = pick();
    if (!handler) return;
    el = e.currentTarget as HTMLElement;
    startX = e.clientX;
    startY = e.clientY;
    captured = false;
    handler.begin();
  }

  function onPointerMove(e: PointerEvent) {
    if (!handler) return;
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    // 按下即捕获会吞掉子元素点击
    if (!captured && Math.hypot(dx, dy) > CAPTURE_SLOP) {
      el?.setPointerCapture(e.pointerId);
      captured = true;
    }
    handler.move(dx, dy);
  }

  function consumeClick(): boolean {
    const swallow = swallowClick;
    swallowClick = false;
    return swallow;
  }

  return {
    onPointerDown,
    onPointerMove,
    onPointerUp: (e: PointerEvent) => finish(e, false),
    onPointerCancel: (e: PointerEvent) => finish(e, true),
    consumeClick,
  };
}
