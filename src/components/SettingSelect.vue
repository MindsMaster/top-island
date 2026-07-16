<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';

export interface SelectOption {
  value: string;
  label: string;
  icon?: string;
  /** 可选分组键；提供 groupLabels 时按组渲染小节标题 */
  group?: string;
}

const props = defineProps<{
  modelValue: string;
  options: SelectOption[];
  /** 分组键 -> 标题文案；缺省则不渲染分组 */
  groupLabels?: Record<string, string>;
}>();

const emit = defineEmits<{ (e: 'update:modelValue', value: string): void }>();

const open = ref(false);
const rootEl = ref<HTMLElement | null>(null);

const current = computed(() => props.options.find((o) => o.value === props.modelValue) ?? props.options[0]);

/** 按选项出现顺序聚合分组（无 groupLabels 时整体为单组、无标题） */
const grouped = computed(() => {
  if (!props.groupLabels) return [{ label: '', options: props.options }];
  const order: string[] = [];
  const byGroup = new Map<string, SelectOption[]>();
  for (const o of props.options) {
    const g = o.group ?? '';
    if (!byGroup.has(g)) {
      byGroup.set(g, []);
      order.push(g);
    }
    byGroup.get(g)!.push(o);
  }
  return order.map((g) => ({ label: props.groupLabels?.[g] ?? '', options: byGroup.get(g)! }));
});

function pick(value: string) {
  emit('update:modelValue', value);
  open.value = false;
}

function onDocMouseDown(e: MouseEvent) {
  if (!rootEl.value?.contains(e.target as Node)) open.value = false;
}

onMounted(() => document.addEventListener('mousedown', onDocMouseDown));
onBeforeUnmount(() => document.removeEventListener('mousedown', onDocMouseDown));
</script>

<template>
  <div ref="rootEl" class="setting-select">
    <button class="setting-select-btn" @click="open = !open">
      <i v-if="current?.icon" :class="'fa-solid ' + current.icon"></i>
      <span>{{ current?.label }}</span>
      <i class="fa-solid fa-chevron-down setting-select-caret" :class="{ open }"></i>
    </button>
    <div v-if="open" class="setting-select-menu">
      <template v-for="(g, gi) in grouped" :key="gi">
        <div v-if="g.label" class="setting-select-group">{{ g.label }}</div>
        <button
          v-for="o in g.options"
          :key="o.value"
          class="setting-select-item"
          :class="{ active: modelValue === o.value }"
          @click="pick(o.value)"
        >
          <i v-if="o.icon" :class="'fa-solid ' + o.icon"></i>
          <span>{{ o.label }}</span>
          <i v-if="modelValue === o.value" class="fa-solid fa-check setting-select-check"></i>
        </button>
      </template>
    </div>
  </div>
</template>
