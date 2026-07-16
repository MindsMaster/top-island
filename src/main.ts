import { createApp } from 'vue';
import App from './App.vue';
import SettingsApp from './SettingsApp.vue';
import '@fortawesome/fontawesome-free/css/all.min.css';
import './styles/island.scss';

const hash = window.location.hash;
const Root = hash.includes('settings') ? SettingsApp : App;
createApp(Root).mount('#app');
