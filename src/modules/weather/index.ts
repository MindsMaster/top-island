import type { IslandModule } from '../types';
import Backdrop from './Backdrop.vue';
import Chip from './Chip.vue';
import Panel from './Panel.vue';
import { startWeather } from './store';

export const weatherModule: IslandModule = {
  id: 'weather',
  setup: startWeather,
  panels: [
    { id: 'weather', icon: 'fa-cloud-sun', titleKey: 'navWeather', component: Panel, backdrop: Backdrop },
  ],
  chip: Chip,
};
