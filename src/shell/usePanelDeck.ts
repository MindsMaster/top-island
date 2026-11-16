import { nextTick, onUnmounted, ref, watch, type CSSProperties, type Ref } from 'vue';
import { animationsSettled } from '@/ui/animations';
import { NO_GESTURE, type DragHandler } from './usePointerDrag';

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
  const mounted = ref(new Set<number>());
  const morphing = ref(false);

  const maxPanel = options.panelCount - 1;

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

  function stopFrame() {
    if (rafId === null) return;
    cancelAnimationFrame(rafId);
    rafId = null;
  }

  const swipe: DragHandler = {
    begin() {
      dragOffset.value = 0;
      isDragging.value = true;
      containerWidth = options.getContainerWidth() || DEFAULT_WIDTH;
    },
    move(dx) {
      // 按帧节流 优化高回报率鼠标场景
      pendingDx = dx;
      if (rafId === null) {
        rafId = requestAnimationFrame(() => {
          rafId = null;
          if (isDragging.value) dragOffset.value = pendingDx;
        });
      }
    },
    end(dx, dy, cancelled) {
      isDragging.value = false;
      stopFrame();
      dragOffset.value = 0;
      if (cancelled || !options.isLarge.value) return;
      if (Math.abs(dx) > Math.abs(dy) && Math.abs(dx) >= SWIPE_THRESHOLD) {
        switchPanel(activePanel.value + (dx < 0 ? 1 : -1));
      }
    },
  };

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
    if (target.closest?.(NO_GESTURE)) return;
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

  onUnmounted(() => {
    if (backfillTimer !== null) clearInterval(backfillTimer);
    stopFrame();
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
    swipe,
    onWheel,
  };
}
