import { reactive, readonly } from 'vue';

export type IslandMode = 'still' | 'quick' | 'large';

export interface ShellView {
  mode: IslandMode;
  /** 缩顶缘留细边 */
  hidden: boolean;
  /** 按 capsule.priority 仲裁 */
  capsuleOwner: string | null;
  /** px 浮层据此定位 */
  islandWidth: number;
  /** px ≤0 浮层随岛 */
  dragOffset: number;
}

const state = reactive<ShellView>({
  mode: 'still',
  hidden: false,
  capsuleOwner: null,
  islandWidth: 170,
  dragOffset: 0,
});

export const shellView = readonly(state);

/** 仅 shell 写 */
export function setShellView(patch: Partial<ShellView>) {
  Object.assign(state, patch);
}
