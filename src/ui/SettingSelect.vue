<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useId } from 'vue';

export interface SelectOption {
  value: string;
  label: string;
  icon?: string;
  /** 分组键 配合 groupLabels */
  group?: string;
}

const props = defineProps<{
  modelValue: string;
  options: SelectOption[];
  /** 分组键到标题 缺省不分组 */
  groupLabels?: Record<string, string>;
  labelledBy?: string;
}>();

const emit = defineEmits<{ (e: 'update:modelValue', value: string): void }>();

const open = ref(false);
const rootEl = ref<HTMLElement | null>(null);
const triggerEl = ref<HTMLButtonElement | null>(null);
const id = useId();

async function toggleMenu() {
  open.value = !open.value;
  if (!open.value) return;
  await nextTick();
  const selected = rootEl.value?.querySelector<HTMLButtonElement>('.setting-select-item.active');
  (selected ?? rootEl.value?.querySelector<HTMLButtonElement>('.setting-select-item'))?.focus();
}

function closeMenu() {
  open.value = false;
  triggerEl.value?.focus();
}

function onMenuKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault();
    event.stopPropagation();
    closeMenu();
    return;
  }
  if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
  event.preventDefault();
  const items = Array.from(rootEl.value?.querySelectorAll<HTMLButtonElement>('.setting-select-item') ?? []);
  const currentIndex = items.indexOf(document.activeElement as HTMLButtonElement);
  const nextIndex =
    event.key === 'Home'
      ? 0
      : event.key === 'End'
        ? items.length - 1
        : (currentIndex + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length;
  items[nextIndex]?.focus();
}

function onFocusOut(event: FocusEvent) {
  if (!rootEl.value?.contains(event.relatedTarget as Node | null)) open.value = false;
}

const current = computed(() => props.options.find((o) => o.value === props.modelValue) ?? props.options[0]);

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
  closeMenu();
}

function onDocMouseDown(e: MouseEvent) {
  if (!rootEl.value?.contains(e.target as Node)) open.value = false;
}

onMounted(() => document.addEventListener('mousedown', onDocMouseDown));
onBeforeUnmount(() => document.removeEventListener('mousedown', onDocMouseDown));
</script>

<template>
  <div ref="rootEl" class="setting-select" @focusout="onFocusOut">
    <button
      ref="triggerEl"
      class="setting-select-btn"
      :aria-labelledby="labelledBy ? `${labelledBy} ${id}-value` : `${id}-value`"
      :title="current?.label"
      aria-haspopup="listbox"
      :aria-expanded="open"
      :aria-controls="open ? `${id}-options` : undefined"
      @click="toggleMenu"
      @keydown.down.prevent="!open && toggleMenu()"
      @keydown.up.prevent="!open && toggleMenu()"
    >
      <i v-if="current?.icon" :class="'fa-solid ' + current.icon" aria-hidden="true"></i>
      <span :id="`${id}-value`">{{ current?.label }}</span>
      <i class="fa-solid fa-chevron-down setting-select-caret" :class="{ open }" aria-hidden="true"></i>
    </button>
    <div
      v-if="open"
      :id="`${id}-options`"
      class="setting-select-menu"
      role="listbox"
      :aria-labelledby="labelledBy ?? `${id}-value`"
      @keydown="onMenuKeydown"
    >
      <template v-for="(g, gi) in grouped" :key="gi">
        <div v-if="g.label" class="setting-select-group">{{ g.label }}</div>
        <button
          v-for="o in g.options"
          :key="o.value"
          class="setting-select-item"
          :class="{ active: modelValue === o.value }"
          :title="o.label"
          role="option"
          :aria-selected="modelValue === o.value"
          tabindex="-1"
          @click="pick(o.value)"
        >
          <i v-if="o.icon" :class="'fa-solid ' + o.icon" aria-hidden="true"></i>
          <span>{{ o.label }}</span>
          <i
            v-if="modelValue === o.value"
            class="fa-solid fa-check setting-select-check"
            aria-hidden="true"
          ></i>
        </button>
      </template>
    </div>
  </div>
</template>
