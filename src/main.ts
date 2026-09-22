import { createApp } from 'vue';
import IslandShell from './shell/IslandShell.vue';
import SettingsApp from './settings/SettingsApp.vue';
import '@fortawesome/fontawesome-free/css/all.min.css';
import './styles/index.scss';

/** 双窗口共用入口 hash 选根组件 */
const Root = window.location.hash.includes('settings') ? SettingsApp : IslandShell;
createApp(Root).mount('#app');
