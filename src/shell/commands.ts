/**
 * 模块能对壳下达的指令。壳在挂载时注册实现，模块（主要是浮层）按需调用，
 * 双方都不必互相 import 组件。
 */
export interface ShellCommands {
  /** 退出上滑隐藏态 */
  reveal(): void;
  /** 展开大视图并切到指定面板 */
  openPanel(panelId: string): void;
}

let impl: ShellCommands | null = null;

export function setShellCommands(c: ShellCommands) {
  impl = c;
}

export const shell: ShellCommands = {
  reveal: () => impl?.reveal(),
  openPanel: (id) => impl?.openPanel(id),
};
