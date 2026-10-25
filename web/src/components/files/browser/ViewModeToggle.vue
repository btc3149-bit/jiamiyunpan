<script setup lang="ts">
import { mdiTable, mdiViewGridOutline } from '@mdi/js'
import BaseButton from '@/components/ui/BaseButton.vue'

const props = defineProps<{
  modelValue: 'table' | 'grid'
}>()

const emits = defineEmits<{
  (event: 'update:modelValue', value: 'table' | 'grid'): void
}>()

const select = (value: 'table' | 'grid') => {
  if (value !== props.modelValue) {
    emits('update:modelValue', value)
  }
}
</script>

<template>
  <div
    class="flex"
    role="group"
    :aria-label="$t('files.viewMode.switch')"
    data-testid="view-mode-toggle"
  >
    <BaseButton
      color="light"
      :xs="true"
      :icon="mdiTable"
      name="view-mode-table"
      :title="$t('files.viewMode.table')"
      :aria-label="$t('files.viewMode.table')"
      :aria-pressed="props.modelValue === 'table'"
      data-testid="view-mode-table"
      @click="select('table')"
    />
    <BaseButton
      color="light"
      :xs="true"
      :icon="mdiViewGridOutline"
      name="view-mode-grid"
      :title="$t('files.viewMode.grid')"
      :aria-label="$t('files.viewMode.grid')"
      :aria-pressed="props.modelValue === 'grid'"
      data-testid="view-mode-grid"
      @click="select('grid')"
    />
  </div>
</template>
