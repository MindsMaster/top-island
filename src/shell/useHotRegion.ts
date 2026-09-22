import { onUnmounted, ref, watch, watchEffect, type Ref } from 'vue';
import { windowApi } from '@/platform/window';

/** 岛形变与浮层进出动画的时长上限 */
const TRANSITION_SETTLE_MS = 550;

/** 模板 ref 可能拿到元素，也可能拿到组件实例（如 TransitionGroup） */
function toElement(v: unknown): Element | null {
  if (v instanceof Element) return v;
  const el = (v as { $el?: unknown } | null)?.$el;
  return el instanceof Element ? el : null;
}

const targets = new Set<Ref<unknown>>();

/** 贡献者增减或尺寸变化时自增，上报逻辑据此重算 */
const version = ref(0);

/**
 * 把一个元素纳入岛窗交互热区。浮层组件自己调用，壳不需要知道有哪些浮层，
 * 也不必按 class 去 DOM 里找它们。组件卸载时自动摘除。
 *
 * 浮层内容随时在变（通知卡涌入、迷你环显隐），所以连尺寸一起盯：
 * 少了这个，新到的卡片会落在热区外，点不动。
 */
export function contributeHotRect(target: Ref<unknown>) {
  targets.add(target);
  const observer = new ResizeObserver(() => version.value++);

  watch(
    target,
    (v) => {
      observer.disconnect();
      const el = toElement(v);
      if (el) observer.observe(el);
      version.value++;
    },
    { immediate: true, flush: 'post' }
  );

  onUnmounted(() => {
    observer.disconnect();
    targets.delete(target);
    version.value++;
  });
}

/** base 与全部贡献矩形的包围盒。零尺寸矩形（隐藏中的浮层）忽略。 */
function unionWithContributors(base: DOMRect): DOMRect {
  let { left, top, right, bottom } = base;
  for (const target of targets) {
    const r = toElement(target.value)?.getBoundingClientRect();
    if (!r || r.width === 0 || r.height === 0) continue;
    left = Math.min(left, r.left);
    top = Math.min(top, r.top);
    right = Math.max(right, r.right);
    bottom = Math.max(bottom, r.bottom);
  }
  return new DOMRect(left, top, right - left, bottom - top);
}

interface HotRectOptions {
  /** 读一遍所有影响热区的响应式量；变化时自动重报 */
  deps(): void;
  /** 全程可交互（拖动中、浮层悬停中）：上报 null 关掉穿透 */
  keepInteractive(): boolean;
  /** 岛本体矩形。浮层的贡献会自动并进来 */
  getIslandRect(): DOMRect | null;
  /** 大视图整窗可交互（点面板外要能收起）时的矩形 */
  getFullRect(): DOMRect | null;
  isFullView(): boolean;
}

/**
 * 把热区上报给 Rust 钩子：穿透与悬停判定都由那边按矩形统一切换。
 * 悬停区恒为岛本体——浮层只要可点，不该触发岛的悬停展开。
 */
export function useHotRectReporter(options: HotRectOptions) {
  function interactiveRect(): DOMRect | null {
    if (options.keepInteractive()) return null;
    if (options.isFullView()) return options.getFullRect() ?? options.getIslandRect();
    const island = options.getIslandRect();
    return island && unionWithContributors(island);
  }

  function report() {
    void windowApi.setHotRect(interactiveRect(), options.getIslandRect()).catch(() => {});
  }

  let settleTimer: number | null = null;

  /** 形变/浮层动画期间矩形还在变，落定后再补一次 */
  function reportSoon() {
    report();
    if (settleTimer !== null) clearTimeout(settleTimer);
    settleTimer = window.setTimeout(() => {
      settleTimer = null;
      report();
    }, TRANSITION_SETTLE_MS);
  }

  watchEffect(() => {
    options.deps();
    void version.value;
    reportSoon();
  });

  // 窗口移动/缩放/DPI 变化只改物理坐标，DOM 矩形不变，也需要重报
  windowApi.onGeometryChanged(report);

  onUnmounted(() => {
    if (settleTimer !== null) clearTimeout(settleTimer);
  });

  return { report };
}
