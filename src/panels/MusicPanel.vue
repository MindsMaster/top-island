<script setup lang="ts">
import { computed, ref } from 'vue';
import { useI18n } from '../i18n';
import { formatTimeMs, useMusic } from '../composables/useMusic';

const { t } = useI18n();
const music = useMusic();

const albumRotation = ref({ x: 0, y: 0 });
const albumHovered = ref(false);

function onAlbumMouseMove(e: MouseEvent) {
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
  const centerX = rect.left + rect.width / 2;
  const centerY = rect.top + rect.height / 2;
  const deltaX = e.clientX - centerX;
  const deltaY = e.clientY - centerY;
  const maxDistance = Math.sqrt(rect.width * rect.width + rect.height * rect.height) / 2;
  albumRotation.value = { x: (deltaY / maxDistance) * 35, y: (deltaX / maxDistance) * -35 };
}

function onAlbumMouseLeave() {
  albumRotation.value = { x: 0, y: 0 };
  albumHovered.value = false;
}

const albumStyle = computed(() => ({
  transform: `rotateX(${albumRotation.value.x}deg) rotateY(${albumRotation.value.y}deg) scale(${albumHovered.value ? 1.25 : 1})`,
  transformStyle: 'preserve-3d' as const,
  transition: 'transform 0.4s cubic-bezier(0.34, 1.56, 0.64, 1), filter 0.3s ease-out',
  filter: albumHovered.value ? 'brightness(1.1)' : 'none',
}));

function onScrubStart() {
  if (music.seekable.value) music.isScrubbing.value = true;
}

function onScrubInput(e: Event) {
  music.positionMs.value = parseInt((e.target as HTMLInputElement).value);
}

function onScrubChange(e: Event) {
  const v = parseInt((e.target as HTMLInputElement).value);
  music.isScrubbing.value = false;
  music.seek(v);
}

function onScrubPointerUp() {
  setTimeout(() => (music.isScrubbing.value = false), 50);
}

const positionLabel = computed(() =>
  music.progressAvailable.value ? formatTimeMs(music.positionMs.value) : '--:--'
);
const durationLabel = computed(() =>
  music.progressAvailable.value ? formatTimeMs(music.effectiveDurationMs.value) : '--:--'
);

const LYRIC_LINE_H = 24;
const lyricsTransform = computed(() => {
  const idx = Math.max(0, music.currentLyricIndex.value);
  return `translateY(${LYRIC_LINE_H - idx * LYRIC_LINE_H}px)`;
});
</script>

<template>
  <div
    class="large-artwork"
    :style="albumStyle"
    @mouseenter="albumHovered = true"
    @mousemove="onAlbumMouseMove"
    @mouseleave="onAlbumMouseLeave"
  >
    <img v-if="music.artworkUrl.value" :src="music.artworkUrl.value" alt="" draggable="false" />
    <i v-else class="fa-solid fa-music"></i>
  </div>
  <div class="large-info">
    <div class="large-title">{{ music.currentTrack.value || t('musicNotPlaying') }}</div>
    <div class="large-artist">{{ music.currentArtist.value }}</div>
  </div>
  <div v-if="music.lyricLines.value.length" class="large-lyrics">
    <div class="lyrics-scroll" :style="{ transform: lyricsTransform }">
      <div
        v-for="(line, i) in music.lyricLines.value"
        :key="i"
        class="lyric-line"
        :class="{ active: i === music.currentLyricIndex.value }"
      >
        {{ line.text }}
      </div>
    </div>
  </div>
  <div class="large-progress" :class="{ disabled: !music.progressAvailable.value }">
    <span class="progress-time">{{ positionLabel }}</span>
    <input
      type="range"
      class="progress-slider-lg"
      :class="{ readonly: !music.seekable.value }"
      min="0"
      :max="music.effectiveDurationMs.value || 100"
      step="1000"
      :value="music.positionMs.value"
      @pointerdown="onScrubStart"
      @pointerup="onScrubPointerUp"
      @input="onScrubInput"
      @change.stop="onScrubChange"
    />
    <span class="progress-time">{{ durationLabel }}</span>
  </div>
  <div class="large-controls">
    <button class="ctrl-btn-lg" @click.stop="music.skipTrack(-1)">
      <i class="fa-solid fa-backward-step"></i>
    </button>
    <button class="ctrl-btn-lg play-btn-lg" @click.stop="music.togglePlay()">
      <i :class="'fa-solid ' + music.playIcon.value"></i>
    </button>
    <button class="ctrl-btn-lg" @click.stop="music.skipTrack(1)">
      <i class="fa-solid fa-forward-step"></i>
    </button>
  </div>
</template>
