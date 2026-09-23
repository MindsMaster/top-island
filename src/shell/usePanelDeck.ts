import { nextTick, onUnmounted, ref, watch, type CSSProperties, type Ref } from 'vue';
import { animationsSettled } from '@/ui/animations';

/** 禁用滑动手势的控件 由控件自行处理 */
const NON_SWIPE_SELECTOR =
  'button, input, textarea, select, .cal-day-cell, .cal-month-cell, .cal-add-all-day, .alarm-quick, .alarm-row, .time-dial, .alarm-editor';

const SCROLL_CAPTURE_SELECTOR = '.alarm-editor, .time-dial';

const SWIPE_THRESHOLD = 40;
const WHEEL_THRESHOLD = 60;
const WHEEL_COOLDOWN_MS = 300;
const WHEEL_IDLE_RESET_MS = 200;

const BACKFILL_MS = 60;
const WARMUP_BACKFILL_MS = 300;

const DEFAULT_WIDTH = 420;

interface PanelDeckOptions {
  panelCount: number;
  isLarge: Ref<boolean>;
  getContainerWidth: () => number;
  getIslandEl: () => HTMLElement | null;
}

export function usePanelDeck(options: PanelDeckOptions) {
  const activePanel = ref(0);
  const dragOffset = ref(0);
  const isDragging = ref(false);
  const suppressClick = ref(false);
  const mounted = ref(new Set<number>());
  const morphing = ref(false);

  const maxPanel = options.panelCount - 1;

  let startX = 0;
  let startY = 0;
  let moved = false;
  let containerWidth = DEFAULT_WIDTH;
  let rafId: number | null = null;
  let pendingDx = 0;
  let morphToken = 0;
  let backfillTimer: number | null = null;

  function panelStyle(i: number): CSSProperties {
    const dragPct = isDragging.value ? (dragOffset.value / containerWidth) * 100 : 0;
    const x = (i - activePanel.value) * 100 + dragPct;
    const dist = Math.abs(x) / 100;
    const far = dist > 1.001;
    return {
      transform: `translate3d(${x}%, 0, 0)`,
      opacity: Math.max(0, 1 - dist * 0.5),
      visibility: far ? 'hidden' : 'visible',
      contentVisibility: far ? 'hidden' : 'visible',
    };
  }

  const isMounted = (i: number) => mounted.value.has(i);
  const isParked = (i: number) => morphing.value && i !== activePanel.value;

  function ensurePanel(i: number) {
    if (i >= 0 && i <= maxPanel && !mounted.value.has(i)) {
      mounted.value = new Set(mounted.value).add(i);
    }
  }

  function startBackfill(intervalMs: number) {
    if (backfillTimer !== null) clearInterval(backfillTimer);
    let next = 0;
    backfillTimer = window.setInterval(() => {
      while (next <= maxPanel && mounted.value.has(next)) next++;
      if (next > maxPanel) {
        clearInterval(backfillTimer!);
        backfillTimer = null;
        return;
      }
      ensurePanel(next);
    }, intervalMs);
  }

  watch(options.isLarge, async (large) => {
    morphing.value = true;
    const token = ++morphToken;
    if (large) {
      ensurePanel(activePanel.value);
      startBackfill(BACKFILL_MS);
    }
    await nextTick();
    await animationsSettled(options.getIslandEl()?.getAnimations() ?? []);
    if (token === morphToken) morphing.value = false;
  });

  // 未挂载的面板 补齐兜底
  watch(activePanel, (i) => {
    if (!options.isLarge.value) return;
    ensurePanel(i - 1);
    ensurePanel(i);
    ensurePanel(i + 1);
  });

  function warmup() {
    startBackfill(WARMUP_BACKFILL_MS);
  }

  function onPointerDown(e: PointerEvent, captureEl?: HTMLElement | null) {
    if (!options.isLarge.value) return;
    if ((e.target as HTMLElement).closest(NON_SWIPE_SELECTOR)) return;
    startX = e.clientX;
    startY = e.clientY;
    moved = false;
    dragOffset.value = 0;
    isDragging.value = true;
    suppressClick.value = false;
    containerWidth = options.getContainerWidth() || DEFAULT_WIDTH;
    (captureEl ?? (e.target as HTMLElement)).setPointerCapture?.(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (!options.isLarge.value || !isDragging.value) return;
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    if (Math.abs(dx) > 5 || Math.abs(dy) > 5) moved = true;
    // 按帧节流 优化高回报率鼠标场景
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
      switchPanel(activePanel.value + (dx < 0 ? 1 : -1));
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
    if (!options.isLarge.value) return;
    const target = e.target as HTMLElement;
    if (target.closest?.(SCROLL_CAPTURE_SELECTOR)) return;
    if (isInScrollable(target)) return;
    const now = Date.now();
    if (now - lastWheelAt > WHEEL_IDLE_RESET_MS) wheelAccum = 0;
    lastWheelAt = now;
    if (now - lastWheelSwitch < WHEEL_COOLDOWN_MS) return;

    wheelAccum += Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
    if (Math.abs(wheelAccum) < WHEEL_THRESHOLD) return;
    const next = activePanel.value + (wheelAccum > 0 ? 1 : -1);
    wheelAccum = 0;
    if (next >= 0 && next <= maxPanel) {
      activePanel.value = next;
      lastWheelSwitch = now;
    }
  }

  function consumeSuppressedClick(): boolean {
    if (!suppressClick.value) return false;
    suppressClick.value = false;
    return true;
  }

  onUnmounted(() => {
    if (backfillTimer !== null) clearInterval(backfillTimer);
    if (rafId !== null) cancelAnimationFrame(rafId);
  });

  return {
    activePanel,
    isDragging,
    morphing,
    panelStyle,
    isMounted,
    isParked,
    warmup,
    switchPanel,
    onPointerDown,
    onPointerMove,
    onPointerUp,
    onWheel,
    consumeSuppressedClick,
  };
}
