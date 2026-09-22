import { onUnmounted, ref, watch, type CSSProperties, type Ref } from 'vue';

/** 命中这些元素时不启动滑动手势，让控件自己处理事件 */
const NON_SWIPE_SELECTOR =
  'button, input, textarea, select, .cal-day-cell, .cal-month-cell, .cal-add-all-day, .alarm-quick, .alarm-row, .time-dial, .alarm-editor';

/** 独占交互区：内部不响应滚轮翻页 */
const SCROLL_CAPTURE_SELECTOR = '.alarm-editor, .time-dial';

/** 滑动触发的最小水平位移（px） */
const SWIPE_THRESHOLD = 40;

/** 滚轮切页：累计滚动量阈值（高精度滚轮/触控板单次事件 delta 很小） */
const WHEEL_THRESHOLD = 60;
/** 冷却（ms），吸收惯性滚动余量，防止一次滚动连跳多页 */
const WHEEL_COOLDOWN_MS = 300;
/** 两次滚轮事件间隔超过该值视为新一轮滚动，重置累计 */
const WHEEL_IDLE_RESET_MS = 200;

/** 岛形变动画时长（与 _shell.scss 的过渡一致） */
const MORPH_MS = 520;

/** 进大视图后补挂剩余面板的间隔；启动后的预热用更长的间隔 */
const BACKFILL_MS = 60;
const WARMUP_BACKFILL_MS = 300;

const DEFAULT_WIDTH = 420;

interface PanelDeckOptions {
  panelCount: number;
  isLarge: Ref<boolean>;
  /** 拖拽起始时读取容器宽度，用于把像素位移换算成百分比 */
  getContainerWidth: () => number;
}

/**
 * 大视图的面板容器：横向滑动/滚轮切页，外加挂载调度。
 *
 * 挂载调度的存在理由：一次挂满全部面板会产生一个几十毫秒的长任务，
 * 展开动画因此掉帧。所以只立刻挂当前页，其余在空闲帧逐个补齐；
 * 挂上之后不再卸载（外层用 v-show），之后展开/收起零挂载成本。
 *
 * 性能要点：pointermove 按 rAF 节流；panelStyle 只输出 transform/opacity，
 * 离屏面板直接 visibility 裁剪。
 */
export function usePanelDeck(options: PanelDeckOptions) {
  const activePanel = ref(0);
  const dragOffset = ref(0);
  const isDragging = ref(false);
  /** 一次有效滑动后抑制紧随的 click（避免误触展开逻辑） */
  const suppressClick = ref(false);
  const mounted = ref(new Set<number>());
  /**
   * 形变进行中。形变每帧改宽高和裁剪区，在场的面板会被迫逐帧重排重绘；
   * 视野外的面板形变期间摘出布局，到位再回来。
   */
  const morphing = ref(false);

  const maxPanel = options.panelCount - 1;

  let startX = 0;
  let startY = 0;
  let moved = false;
  let containerWidth = DEFAULT_WIDTH;
  let rafId: number | null = null;
  let pendingDx = 0;
  let morphTimer: number | null = null;
  let backfillTimer: number | null = null;

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

  watch(options.isLarge, (large) => {
    morphing.value = true;
    if (morphTimer !== null) clearTimeout(morphTimer);
    morphTimer = window.setTimeout(() => {
      morphing.value = false;
      morphTimer = null;
    }, MORPH_MS);

    if (!large) return;
    ensurePanel(activePanel.value);
    startBackfill(BACKFILL_MS);
  });

  // 滑到还没挂上的面板时立即补上（补齐通常早已完成，这是兜底）
  watch(activePanel, (i) => {
    if (!options.isLarge.value) return;
    ensurePanel(i - 1);
    ensurePanel(i);
    ensurePanel(i + 1);
  });

  /** 启动后趁空闲把面板慢慢挂好，第一次展开就不需要现挂 */
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
    // 高回报率鼠标的 pointermove 可达数百 Hz，按帧节流
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

  /** 目标处在可滚动列表内（且列表确实有内容可滚）时，滚轮让给列表 */
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
    if (!suppressClick.value) return false;
    suppressClick.value = false;
    return true;
  }

  onUnmounted(() => {
    if (backfillTimer !== null) clearInterval(backfillTimer);
    if (morphTimer !== null) clearTimeout(morphTimer);
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
