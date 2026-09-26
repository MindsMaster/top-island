import type { Component } from 'vue';
import AboutSection from './sections/AboutSection.vue';
import AppearanceSection from './sections/AppearanceSection.vue';
import DiagnosticsSection from './sections/DiagnosticsSection.vue';
import GeneralSection from './sections/GeneralSection.vue';
import MessagesSection from './sections/MessagesSection.vue';
import MusicSection from './sections/MusicSection.vue';
import WeatherSection from './sections/WeatherSection.vue';

export interface SettingsSection {
  id: string;
  icon: string;
  titleKey: string;
  component: Component;
  bottom?: boolean;
}

export const sections: SettingsSection[] = [
  { id: 'general', icon: 'fa-sliders', titleKey: 'settingsGeneral', component: GeneralSection },
  {
    id: 'appearance',
    icon: 'fa-palette',
    titleKey: 'settingsAppearance',
    component: AppearanceSection,
  },
  { id: 'weather', icon: 'fa-cloud-sun', titleKey: 'settingsWeather', component: WeatherSection },
  { id: 'messages', icon: 'fa-bell', titleKey: 'settingsMessages', component: MessagesSection },
  { id: 'music', icon: 'fa-music', titleKey: 'settingsMusic', component: MusicSection },
  { id: 'diag', icon: 'fa-gear', titleKey: 'settingsDiag', component: DiagnosticsSection },
  {
    id: 'about',
    icon: 'fa-circle-info',
    titleKey: 'settingsAbout',
    component: AboutSection,
    bottom: true,
  },
];
