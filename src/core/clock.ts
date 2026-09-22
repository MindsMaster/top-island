import { computed, ref } from 'vue';
import { useI18n } from './i18n';

const { lang, weekdays } = useI18n();

const now = ref(new Date());
let timer: number | null = null;

/** 界面只显示到分钟 */
export function startClock() {
  if (timer !== null) return;
  now.value = new Date();
  timer = window.setInterval(() => (now.value = new Date()), 30000);
}

export { now };

export const currentTime = computed(
  () => `${now.value.getHours()}:${String(now.value.getMinutes()).padStart(2, '0')}`
);

const EN_MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

export const currentDate = computed(() => {
  const n = now.value;
  const wd = weekdays.value;
  if (lang.value === 'en-US') {
    return `${wd[n.getDay()]} ${EN_MONTHS[n.getMonth()]} ${n.getDate()}`;
  }
  return `${n.getMonth() + 1}月${n.getDate()}日 周${wd[n.getDay()]}`;
});

export const isNightTime = computed(() => {
  const h = now.value.getHours();
  return h >= 18 || h < 6;
});

/** 本地日期 勿换 toISOString */
export function fmtLocalDate(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

export function todayLocalStr(): string {
  return fmtLocalDate(new Date());
}
