import { computed, ref, watch } from 'vue';
import { shellView } from '@/shell/view';
import { alarmState } from './store';

/** 悬停迷你环时岛须可交互 */
export const miniHovered = ref(false);

export const showMini = computed(
  () =>
    alarmState.countdown.running &&
    shellView.mode !== 'large' &&
    (shellView.capsuleOwner !== 'alarm' || shellView.hidden)
);

watch(showMini, (visible) => {
  if (!visible) miniHovered.value = false;
});
