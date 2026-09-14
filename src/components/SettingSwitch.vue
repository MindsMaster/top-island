<script setup lang="ts">
import { useId } from 'vue';

defineProps<{
  modelValue: boolean;
  label: string;
  description?: string;
}>();

const emit = defineEmits<{ 'update:modelValue': [value: boolean] }>();
const id = useId();
</script>

<template>
  <div class="setting-row">
    <div class="setting-label" :title="description">
      <label :id="`${id}-label`" :for="id">{{ label }}</label>
    </div>
    <slot />
    <button
      :id="id"
      type="button"
      class="setting-toggle"
      :class="{ on: modelValue }"
      role="switch"
      :aria-checked="modelValue"
      :aria-labelledby="`${id}-label`"
      :aria-description="description"
      :title="description"
      @click="emit('update:modelValue', !modelValue)"
    >
      <span class="setting-toggle-knob"></span>
    </button>
  </div>
</template>
