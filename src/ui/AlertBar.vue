<script setup lang="ts">
import { alertState, dismissAlert } from './alert';

/** 点动作按钮后立即收起；固定时长只是不点时的兜底 */
function run(handler: (() => void) | null) {
  handler?.();
  dismissAlert();
}
</script>

<template>
  <div class="alert-content" @click.stop="alertState.actionHandler ? null : dismissAlert()">
    <i :class="'fa-solid ' + alertState.icon"></i>
    <span class="alert-text">{{ alertState.text }}</span>
    <button
      v-if="alertState.secondLabel"
      class="alert-action-btn secondary"
      @click.stop="run(alertState.secondHandler)"
    >
      {{ alertState.secondLabel }}
    </button>
    <button
      v-if="alertState.actionLabel"
      class="alert-action-btn"
      @click.stop="run(alertState.actionHandler)"
    >
      {{ alertState.actionLabel }}
    </button>
    <i v-if="alertState.dismissible" class="fa-solid fa-xmark alert-close" @click.stop="dismissAlert()"></i>
  </div>
</template>
