import { ref } from 'vue';
import type { CSSProperties } from 'vue';

interface PanelSwipeOptions {
  panelCount: number;
  /** 手势是否生效（仅 large 视图） */
  isEnabled: () => boolean;
  /** 拖拽起始时读取容器宽度，用于把像素位移换算成百分比 */
  getContainerWidth: () => number;
}

/** 命中这些元素时不启动滑动手势，让控件自己处理事件 */
const NON_SWIPE_SELECTOR =
  'button, input, textarea, select, .cal-day-cell, .cal-month-cell, .cal-add-all-day, .alarm-quick, .alarm-row, .time-dial, .alarm-editor';

/** 滑动触发的最小水平位移（px） */
const SWIPE_THRESHOLD = 40;

/** 滚轮切页：累计滚动量阈值（高精度滚轮/触控板单次事件 delta 很小） */
const WHEEL_THRESHOLD = 60;
/** 滚轮切页冷却（ms），吸收惯性滚动的余量，防止一次滚动连跳多页 */
const WHEEL_COOLDOWN_MS = 300;
/** 两次滚轮事件间隔超过该值视为新一轮滚动，重置累计 */
const WHEEL_IDLE_RESET_MS = 200;

/**
 * 大视图的面板横向滑动手势。
 * 性能要点：pointermove 按 rAF 节流写入响应式状态；
 * panelStyle 只输出 transform/opacity（合成器友好），离屏面板直接 visibility 裁剪。
 */
export function usePanelSwipe(options: PanelSwipeOptions) {
  const activePanel = ref(0);
  const dragOffset = ref(0);
  const isDragging = ref(false);
  /** 一次有效滑动后抑制紧随的 click（避免误触展开逻辑） */
  const suppressClick = ref(false);

  const maxPanel = options.panelCount - 1;

  let startX = 0;
  let startY = 0;
  let moved = false;
  let containerWidth = 420;
  let rafId: number | null = null;
  let pendingDx = 0;

  function panelStyle(i: number): CSSProperties {
    const dragPct = isDragging.value ? (dragOffset.value / containerWidth) * 100 : 0;
    const x = (i - activePanel.value) * 100 + dragPct;
    const dist = Math.abs(x) / 100;
    return {
      transform: `translate3d(${x}%, 0, 0)`,
      opacity: Math.max(0, 1 - dist * 0.5),
      // 完全滑出容器的面板不再参与绘制与合成
      visibility: dist > 1.001 ? 'hidden' : 'visible',
    };
  }

  function onPointerDown(e: PointerEvent, captureEl?: HTMLElement | null) {
    if (!options.isEnabled()) return;
    if ((e.target as HTMLElement).closest(NON_SWIPE_SELECTOR)) return;
    startX = e.clientX;
    startY = e.clientY;
    moved = false;
    dragOffset.value = 0;
    isDragging.value = true;
    suppressClick.value = false;
    containerWidth = options.getContainerWidth() || 420;
    (captureEl ?? (e.target as HTMLElement)).setPointerCapture?.(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (!options.isEnabled() || !isDragging.value) return;
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    if (Math.abs(dx) > 5 || Math.abs(dy) > 5) moved = true;
    // 高回报率鼠标的 pointermove 频率可达数百 Hz，按帧节流
    pendingDx = dx;
    if (rafId === null) {
      rafId = requestAnimationFrame(() => {
        rafId = null;
        if (isDragging.value) dragOffset.value = pendingDx;
      });
    }
  }

  function onPointerUp(e: PointerEvent) {
    if (!isDragging.value) return;
    isDragging.value = false;
    if (rafId !== null) {
      cancelAnimationFrame(rafId);
      rafId = null;
    }
    (e.target as HTMLElement).releasePointerCapture?.(e.pointerId);
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    if (moved && Math.abs(dx) > Math.abs(dy) && Math.abs(dx) >= SWIPE_THRESHOLD) {
      const next = activePanel.value + (dx < 0 ? 1 : -1);
      if (next >= 0 && next <= maxPanel) activePanel.value = next;
      suppressClick.value = true;
    }
    dragOffset.value = 0;
  }

  function switchPanel(idx: number) {
    if (idx < 0 || idx > maxPanel) return;
    activePanel.value = idx;
  }

  let wheelAccum = 0;
  let lastWheelAt = 0;
  let lastWheelSwitch = 0;

  /** 目标处于可滚动列表内（且列表确实有内容可滚）时滚轮让给列表 */
  function isInScrollable(target: HTMLElement | null): boolean {
    for (let el = target; el; el = el.parentElement) {
      if (el.classList?.contains('panel')) break;
      if (el.scrollHeight > el.clientHeight + 1) {
        const oy = getComputedStyle(el).overflowY;
        if (oy === 'auto' || oy === 'scroll') return true;
      }
    }
    return false;
  }

  function onWheel(e: WheelEvent) {
    if (!options.isEnabled()) return;
    // 编辑抽屉等独占交互区内不响应滚轮翻页
    if ((e.target as HTMLElement).closest?.('.alarm-editor, .time-dial')) return;
    if (isInScrollable(e.target as HTMLElement)) return;
    const now = Date.now();
    if (now - lastWheelAt > WHEEL_IDLE_RESET_MS) wheelAccum = 0;
    lastWheelAt = now;
    if (now - lastWheelSwitch < WHEEL_COOLDOWN_MS) return;
    // 支持横向滚轮/触控板横扫，取主导轴
    wheelAccum += Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
    if (Math.abs(wheelAccum) < WHEEL_THRESHOLD) return;
    const next = activePanel.value + (wheelAccum > 0 ? 1 : -1);
    wheelAccum = 0;
    if (next >= 0 && next <= maxPanel) {
      activePanel.value = next;
      lastWheelSwitch = now;
    }
  }

  /** 消费一次 click 抑制标记；返回 true 表示本次 click 应被忽略 */
  function consumeSuppressedClick(): boolean {
    if (suppressClick.value) {
      suppressClick.value = false;
      return true;
    }
    return false;
  }

  return {
    activePanel,
    isDragging,
    panelStyle,
    onPointerDown,
    onPointerMove,
    onPointerUp,
    onWheel,
    switchPanel,
    consumeSuppressedClick,
  };
}
