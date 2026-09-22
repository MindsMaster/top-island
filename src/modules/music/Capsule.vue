<script setup lang="ts">
import Marquee from '@/ui/Marquee.vue';
import { shellView } from '@/shell/view';
import { currentLyric, marqueeText, musicState as snap } from './store';
</script>

<template>
  <div class="quick-content music-full">
    <div class="artwork-wrap">
      <img v-if="snap.artworkUrl" :src="snap.artworkUrl" alt="" draggable="false" />
      <i v-else class="fa-solid fa-music"></i>
    </div>
    <div v-if="currentLyric" class="lyric-box">
      <span :key="currentLyric" class="lyric-line">{{ currentLyric }}</span>
    </div>
    <Marquee v-else :text="marqueeText" :active="snap.isPlaying && shellView.mode === 'still'" />
  </div>
</template>
