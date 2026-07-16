import { ref } from 'vue';

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

const active = ref(false);
const text = ref('');
const icon = ref('fa-bell');
const dismissible = ref(true);
const actionLabel = ref('');
const actionHandler = ref<(() => void) | null>(null);
const secondLabel = ref('');
const secondHandler = ref<(() => void) | null>(null);
let timer: number | null = null;

function show({
  icon: i = 'fa-bell',
  text: t = '',
  dismissible: d = true,
  duration = 5000,
  actionLabel: al = '',
  actionHandler: ah = null,
  secondLabel: sl = '',
  secondHandler: sh = null,
}: AlertOptions) {
  icon.value = i;
  text.value = t;
  dismissible.value = d;
  actionLabel.value = al;
  actionHandler.value = ah;
  secondLabel.value = sl;
  secondHandler.value = sh;
  active.value = true;
  if (timer) clearTimeout(timer);
  if (duration > 0) timer = window.setTimeout(dismiss, duration);
}

function dismiss() {
  active.value = false;
  actionLabel.value = '';
  actionHandler.value = null;
  secondLabel.value = '';
  secondHandler.value = null;
  if (timer) {
    clearTimeout(timer);
    timer = null;
  }
}

export function useAlert() {
  return {
    active,
    text,
    icon,
    dismissible,
    actionLabel,
    actionHandler,
    secondLabel,
    secondHandler,
    show,
    dismiss,
  };
}
