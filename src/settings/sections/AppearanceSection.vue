<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { systemApi } from '@/platform/system';
import { useI18n } from '@/core/i18n';
import { setCustomColor, settings } from '@/core/settings';
import { THEMES, isLightCustom } from '@/core/theme';
import ColorPicker from '@/ui/ColorPicker.vue';
import SettingSelect from '@/ui/SettingSelect.vue';
import type { CustomTheme, DisplayInfo, ThemeId } from '@/platform/types';

const { t } = useI18n();

const themeOptions = computed(() =>
  THEMES.map((tm) => ({ value: tm.id, label: t(tm.nameKey), icon: tm.icon, group: tm.group }))
);

const themeGroupLabels = computed(() => ({
  solid: t('themeGroupSolid'),
  gradient: t('themeGroupGradient'),
  custom: t('themeGroupCustom'),
}));

const COLOR_PARTS: Array<{ key: keyof CustomTheme; labelKey: string }> = [
  { key: 'a', labelKey: 'customColorA' },
  { key: 'b', labelKey: 'customColorB' },
  { key: 'accent', labelKey: 'customColorAccent' },
];

/** 一次只展开一个取色器 */
const openPicker = ref<keyof CustomTheme | null>(null);

function togglePicker(key: keyof CustomTheme) {
  openPicker.value = openPicker.value === key ? null : key;
}

function closePickerOnOutsideClick(e: MouseEvent) {
  if (!(e.target as HTMLElement).closest('.custom-color-item')) openPicker.value = null;
}

onMounted(() => document.addEventListener('mousedown', closePickerOnOutsideClick));
onBeforeUnmount(() => document.removeEventListener('mousedown', closePickerOnOutsideClick));

const previewTextColor = computed(() =>
  isLightCustom(settings.customTheme) ? 'rgba(0, 0, 0, 0.85)' : '#fff'
);

const displays = ref<DisplayInfo[]>([]);

const displayOptions = computed(() => {
  const counts = new Map<string, number>();
  for (const d of displays.value) counts.set(d.label, (counts.get(d.label) ?? 0) + 1);
  // 同型号名相同 附 GDI 名区分
  return displays.value.map((d) => ({
    value: d.id,
    label: counts.get(d.label)! > 1 ? `${d.label} (${d.id.replace(/^\\+\.\\/, '')})` : d.label,
    icon: 'fa-display',
  }));
});

const displayValue = computed(() =>
  settings.island.displayId === 'primary'
    ? (displays.value.find((d) => d.primary)?.id ?? '')
    : settings.island.displayId
);

function onDisplayPick(id: string) {
  // 主显示器存 primary 抗重插拔
  settings.island.displayId = displays.value.find((d) => d.id === id)?.primary ? 'primary' : id;
}

const SCALE_MIN = 45;
const SCALE_MAX = 300;

function onScaleTyped(e: Event) {
  const el = e.target as HTMLInputElement;
  const v = parseInt(el.value.replace(/\D/g, ''));
  const next = Number.isFinite(v) ? Math.max(SCALE_MIN, Math.min(SCALE_MAX, v)) : settings.island.scale;
  settings.island.scale = next;
  el.value = String(next);
}

function onPeekInput(e: Event) {
  settings.island.hiddenPeek = parseInt((e.target as HTMLInputElement).value);
}

onMounted(async () => {
  displays.value = await systemApi.displays().catch(() => []);
});
</script>

<template>
  <section class="setting-group">
    <div class="setting-row">
      <label id="theme-label" class="setting-label">{{ t('settingsTheme') }}</label>
      <SettingSelect
        :model-value="settings.theme"
        :options="themeOptions"
        :group-labels="themeGroupLabels"
        labelled-by="theme-label"
        @update:model-value="settings.theme = $event as ThemeId"
      />
    </div>
    <div v-if="settings.theme === 'custom'" class="setting-row custom-palette">
      <div class="custom-preview" aria-hidden="true">
        <div
          class="custom-preview-pill"
          :style="{
            background: `linear-gradient(135deg, ${settings.customTheme.a}, ${settings.customTheme.b})`,
          }"
        >
          <i class="fa-solid fa-music" :style="{ color: settings.customTheme.accent }"></i>
          <span :style="{ color: previewTextColor }">12:34</span>
        </div>
      </div>
      <div class="custom-swatches">
        <div v-for="p in COLOR_PARTS" :key="p.key" class="custom-color-item">
          <button
            class="custom-swatch"
            :class="{ open: openPicker === p.key }"
            :style="{ background: settings.customTheme[p.key] }"
            :aria-label="t(p.labelKey)"
            :aria-expanded="openPicker === p.key"
            @click="togglePicker(p.key)"
          ></button>
          <span class="custom-color-label">{{ t(p.labelKey) }}</span>
          <div v-if="openPicker === p.key" class="cp-popover" @keydown.esc.stop="openPicker = null">
            <ColorPicker
              :model-value="settings.customTheme[p.key]"
              @update:model-value="setCustomColor(p.key, $event)"
            />
          </div>
        </div>
      </div>
    </div>
  </section>
  <section class="setting-section">
    <h2 class="setting-group-title">{{ t('settingsLayoutTitle') }}</h2>
    <div class="setting-group">
      <div v-if="displays.length > 1" class="setting-row">
        <label id="display-label" class="setting-label">{{ t('settingsDisplayMonitor') }}</label>
        <SettingSelect
          :model-value="displayValue"
          :options="displayOptions"
          labelled-by="display-label"
          @update:model-value="onDisplayPick"
        />
      </div>
      <div class="setting-row">
        <label for="island-scale" class="setting-label">{{ t('settingsIslandScale') }}</label>
        <div class="setting-num">
          <input
            id="island-scale"
            class="setting-num-input"
            inputmode="numeric"
            maxlength="3"
            :value="settings.island.scale"
            spellcheck="false"
            @focus="($event.target as HTMLInputElement).select()"
            @mouseup.prevent="($event.target as HTMLInputElement).select()"
            @keydown.enter="($event.target as HTMLInputElement).blur()"
            @blur="onScaleTyped"
          />
          <span class="setting-num-unit">%</span>
        </div>
      </div>
      <div class="setting-row offset-row">
        <label for="hidden-peek" class="setting-label">{{ t('settingsHiddenPeek') }}</label>
        <div class="offset-control">
          <input
            id="hidden-peek"
            type="range"
            class="offset-slider"
            min="2"
            max="20"
            step="1"
            :value="settings.island.hiddenPeek"
            @input="onPeekInput"
          />
          <span class="offset-value">{{ settings.island.hiddenPeek }} px</span>
        </div>
      </div>
    </div>
  </section>
</template>
