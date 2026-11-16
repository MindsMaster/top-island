import { ref } from 'vue';
import type { DragHandler } from './usePointerDrag';

const MAX_OFFSET = 44;
const TRIGGER_DISTANCE = 24;

export function useHideGesture(onHide: () => void) {
  const dragging = ref(false);
  const offset = ref(0);

  const handler: DragHandler = {
    begin() {
      dragging.value = true;
      offset.value = 0;
    },
    move(_dx, dy) {
      offset.value = Math.max(-MAX_OFFSET, Math.min(0, dy));
    },
    end(dx, dy, cancelled) {
      dragging.value = false;
      offset.value = 0;
      if (!cancelled && dy < -TRIGGER_DISTANCE && Math.abs(dy) > Math.abs(dx)) onHide();
    },
  };

  return { dragging, offset, handler };
}
