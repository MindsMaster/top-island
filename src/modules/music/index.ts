import { shellView } from '@/shell/view';
import type { IslandModule } from '../types';
import Capsule from './Capsule.vue';
import Panel from './Panel.vue';
import { musicState, startMusicSync } from './store';
import { capsuleWidth } from './width';

export const musicModule: IslandModule = {
  id: 'music',
  setup: startMusicSync,
  panels: [{ id: 'music', icon: 'fa-music', titleKey: 'navMusic', component: Panel }],
  capsule: {
    priority: 20,
    active: () => musicState.hasMusic && musicState.isPlaying && shellView.mode === 'still',
    width: capsuleWidth,
    component: Capsule,
  },
};
