import { reactive } from 'vue';

export interface AlertOptions {
  icon?: string;
  text?: string;
  dismissible?: boolean;
  duration?: number;
  actionLabel?: string;
  actionHandler?: (() => void) | null;
  /** 次要动作（如响铃条的"延后"），显示在主动作之前 */
  secondLabel?: string;
  secondHandler?: (() => void) | null;
}

/** 岛内提示条（更新就绪、响铃等）。handler 是函数，reactive 原样存不包装 */
export const alertState = reactive({
  active: false,
  text: '',
  icon: 'fa-bell',
  dismissible: true,
  actionLabel: '',
  actionHandler: null as (() => void) | null,
  secondLabel: '',
  secondHandler: null as (() => void) | null,
});

let timer: number | null = null;

export function showAlert({
  icon: i = 'fa-bell',
  text: t = '',
  dismissible: d = true,
  duration = 5000,
  actionLabel: al = '',
  actionHandler: ah = null,
  secondLabel: sl = '',
  secondHandler: sh = null,
}: AlertOptions) {
  alertState.icon = i;
  alertState.text = t;
  alertState.dismissible = d;
  alertState.actionLabel = al;
  alertState.actionHandler = ah;
  alertState.secondLabel = sl;
  alertState.secondHandler = sh;
  alertState.active = true;
  if (timer) clearTimeout(timer);
  if (duration > 0) timer = window.setTimeout(dismissAlert, duration);
}

export function dismissAlert() {
  alertState.active = false;
  alertState.actionLabel = '';
  alertState.actionHandler = null;
  alertState.secondLabel = '';
  alertState.secondHandler = null;
  if (timer) {
    clearTimeout(timer);
    timer = null;
  }
}
