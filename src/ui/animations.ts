/** 被打断的动画 finished 会 reject 也算结束 */
export function animationsSettled(animations: Animation[]): Promise<unknown> {
  return Promise.allSettled(
    animations.filter((a) => a.effect?.getComputedTiming().endTime !== Infinity).map((a) => a.finished)
  );
}
