import type { Component } from 'vue';

export interface PanelSlot {
  /** 同时作为面板容器 class `<id>-panel` 样式据此选中 */
  id: string;
  /** FontAwesome 类名 不含 fa-solid 前缀 */
  icon: string;
  titleKey: string;
  component: Component;
  /** 铺满整座岛 随本页淡入淡出 */
  backdrop?: Component;
}

/** 收起态独占整条胶囊 同时想要的模块按 priority 取最大者 */
export interface CapsuleSlot {
  priority: number;
  /** 只能读 shellView.mode/hidden 不能读 capsuleOwner */
  active(): boolean;
  /** 想要的岛宽度 px null 用样式默认值 */
  width?(): number | null;
  component: Component;
}

export interface IslandModule {
  id: string;

  setup?(): void | Promise<void>;

  panels?: PanelSlot[];

  capsule?: CapsuleSlot;

  /** quick 态状态栏挂件 自行决定显隐 */
  chip?: Component;

  /** 岛外浮层 常驻挂载 自行调 contributeHotRect 上报热区 */
  overlay?: Component;

  /** 展开大视图时想跳到自己的哪一页 多个模块想跳按 registry 顺序取第一个 */
  expandTarget?(): string | null;

  /** 岛必须全程可点 关掉鼠标穿透 */
  keepInteractive?(): boolean;

  /** 悬停时不要切到 quick 态 本模块当前内容更重要 */
  holdContent?(): boolean;
}
