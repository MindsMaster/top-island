import { computed, ref } from 'vue';
import { useI18n } from '../i18n';

const now = ref(new Date());
let timer: number | null = null;

export function startClock() {
  if (timer !== null) return;
  now.value = new Date();
  timer = window.setInterval(() => (now.value = new Date()), 30000);
}

export function useClock() {
  const { lang, weekdays } = useI18n();

  const currentTime = computed(() => {
    const n = now.value;
    return n.getHours() + ':' + String(n.getMinutes()).padStart(2, '0');
  });

  const currentDate = computed(() => {
    const n = now.value;
    const wd = weekdays.value;
    if (lang.value === 'en-US') {
      const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
      return `${wd[n.getDay()]} ${months[n.getMonth()]} ${n.getDate()}`;
    }
    return `${n.getMonth() + 1}月${n.getDate()}日 周${wd[n.getDay()]}`;
  });

  const isNightTime = computed(() => {
    const h = now.value.getHours();
    return h >= 18 || h < 6;
  });

  return { now, currentTime, currentDate, isNightTime, startClock };
}

/** 本地日期字符串 YYYY-MM-DD */
export function fmtLocalDate(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

export function todayLocalStr(): string {
  return fmtLocalDate(new Date());
}
