import { nextTick, onUnmounted, ref, watch, watchEffect, type Ref } from 'vue';
import { windowApi } from '@/platform/window';
import { animationsSettled } from '@/ui/animations';

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
  getIslandEl(): Element | null;
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

  let frame: number | null = null;
  let lastSent = '';

  function flush() {
    frame = null;
    const interactive = interactiveRect();
    const hover = options.getIslandRect();
    const key = JSON.stringify([interactive, hover]);
    if (key === lastSent) return;
    lastSent = key;
    void windowApi.setHotRect(interactive, hover).catch(() => {
      lastSent = '';
    });
  }

  function report() {
    if (frame === null) frame = requestAnimationFrame(flush);
  }

  function resend() {
    lastSent = '';
    report();
  }

  let settleToken = 0;

  // 过渡中途的矩形不作数 落定后补报
  async function reportSoon() {
    report();
    const token = ++settleToken;
    await nextTick();
    const els = [options.getIslandEl(), ...[...targets].map((t) => toElement(t.value))];
    await animationsSettled(els.flatMap((el) => el?.getAnimations() ?? []));
    if (token === settleToken) report();
  }

  watchEffect(() => {
    options.deps();
    void version.value;
    void reportSoon();
  });

  // 窗口移动缩放改物理坐标 DOM 矩形不变
  windowApi.onGeometryChanged(resend);

  onUnmounted(() => {
    settleToken++;
    if (frame !== null) cancelAnimationFrame(frame);
  });

  return { report };
}
