import { computed, ref, watch } from 'vue';
import { shellView } from '@/shell/view';
import { alarmState } from './store';

/** 指针停在迷你环上时岛必须可点（否则点不到停止按钮） */
export const miniHovered = ref(false);

/**
 * 倒计时在跑，但胶囊被优先级更高的模块占走（或岛被上滑隐藏）时，
 * 降级成岛旁边的迷你环。
 */
export const showMini = computed(
  () =>
    alarmState.countdown.running &&
    shellView.mode !== 'large' &&
    (shellView.capsuleOwner !== 'alarm' || shellView.hidden)
);

watch(showMini, (visible) => {
  if (!visible) miniHovered.value = false;
});
