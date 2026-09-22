import { reactive, readonly } from 'vue';

export type IslandMode = 'still' | 'quick' | 'large';

export interface ShellView {
  mode: IslandMode;
  /** 上滑隐藏态：岛缩到屏幕顶缘只留一道细边 */
  hidden: boolean;
  /** 当前占据胶囊的模块 id；无人占据为 null。由壳按 capsule.priority 算出 */
  capsuleOwner: string | null;
  /** 岛本体当前宽度（px）。浮层据此把自己摆在岛旁边 */
  islandWidth: number;
  /** 上滑手势的跟手位移（px，<= 0）；非拖动中为 0。浮层要跟着岛一起走 */
  dragOffset: number;
}

const state = reactive<ShellView>({
  mode: 'still',
  hidden: false,
  capsuleOwner: null,
  islandWidth: 170,
  dragOffset: 0,
});

/** 模块读它判断自己该呈现成什么形态 */
export const shellView = readonly(state);

/** 只有 shell 自己写 */
export function setShellView(patch: Partial<ShellView>) {
  Object.assign(state, patch);
}
