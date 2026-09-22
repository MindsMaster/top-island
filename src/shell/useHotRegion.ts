import { onUnmounted, ref, watch, watchEffect, type Ref } from 'vue';
import { windowApi } from '@/platform/window';

const TRANSITION_SETTLE_MS = 550;

/** 模板 ref 可能拿到组件实例 */
function toElement(v: unknown): Element | null {
  if (v instanceof Element) return v;
  const el = (v as { $el?: unknown } | null)?.$el;
  return el instanceof Element ? el : null;
}

const targets = new Set<Ref<unknown>>();

const version = ref(0);

export function contributeHotRect(target: Ref<unknown>) {
  targets.add(target);
  // 浮层内容变化也要重报 否则新内容落在热区外点不动
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
  /** 读一遍影响热区的响应式量 */
  deps(): void;
  keepInteractive(): boolean;
  getIslandRect(): DOMRect | null;
  getFullRect(): DOMRect | null;
  isFullView(): boolean;
}

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

  // 窗口移动缩放改物理坐标 DOM 矩形不变
  windowApi.onGeometryChanged(report);

  onUnmounted(() => {
    if (settleTimer !== null) clearTimeout(settleTimer);
  });

  return { report };
}
