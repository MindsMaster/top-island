import type { Component } from 'vue';

/**
 * 大视图里的一页。模块可以贡献多页（如任务模块的清单页和日历页），
 * 页序 = 模块在 registry 里的顺序 + 模块内的声明顺序。
 */
export interface PanelSlot {
  /** 同时作为面板容器 class（`<id>-panel`），供样式选择器使用 */
  id: string;
  /** FontAwesome 图标类（不含 fa-solid 前缀） */
  icon: string;
  /** 指示器 tooltip 的 i18n key */
  titleKey: string;
  component: Component;
}

/**
 * 收起态独占整条胶囊（歌词、任务提醒、倒计时）。这是唯一需要壳仲裁的槽位：
 * 同时想要的模块按 priority 取最大者。active() 只能读 shellView.mode/hidden，
 * 不能读 capsuleOwner——那正是由它算出来的。
 */
export interface CapsuleSlot {
  priority: number;
  active(): boolean;
  /** 想要的岛宽度（px）；返回 null 用样式里的默认宽度 */
  width?(): number | null;
  component: Component;
}

/**
 * 一个功能的全部对外形态。壳只认这个接口，不认识任何具体模块；
 * 新增功能 = 新建一个目录 + 在 registry 里加一行。
 */
export interface IslandModule {
  id: string;

  /** 启动时由壳统一调用，模块之间顺序无关 */
  setup?(): void | Promise<void>;

  panels?: PanelSlot[];

  capsule?: CapsuleSlot;

  /** quick 态状态栏里的小挂件（如天气温度）。自行决定显隐。 */
  chip?: Component;

  /**
   * 岛本体之外的浮层（通知栈、迷你倒计时环）。常驻挂载，
   * 自行决定显隐与过渡，并用 contributeHotRect 上报自己的热区。
   */
  overlay?: Component;

  /**
   * 展开大视图时本模块想跳到自己的哪一页；不关心则返回 null。
   * 多个模块都想跳时按 registry 顺序取第一个。
   */
  expandTarget?(): string | null;

  /** 岛必须全程可点（关闭鼠标穿透），例如指针正悬在本模块的浮层上 */
  keepInteractive?(): boolean;

  /** 悬停时不要切到 quick 态：本模块当前显示的内容更重要 */
  holdContent?(): boolean;
}
