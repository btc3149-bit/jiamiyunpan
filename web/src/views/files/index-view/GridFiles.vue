<script setup lang="ts">
import FileGridCard from './FileGridCard.vue'
import type { AppFile } from 'types'
import { ref } from 'vue'

const props = defineProps<{
  items: AppFile[]
  checkedIds: Set<string>
  hideDelete?: boolean
  share?: boolean
  hideCheckbox?: boolean
  searchedFileId?: string
}>()

const emits = defineEmits<{
  (event: 'actions', file: AppFile): void
  (event: 'deselect-all'): void
  (event: 'details', file: AppFile): void
  (event: 'download', file: AppFile): void
  (event: 'remove', file: AppFile): void
  (event: 'rename', file: AppFile): void
  (event: 'sharing', file: AppFile): void
  (event: 'fork', file: AppFile): void
  (event: 'leave', file: AppFile): void
  (event: 'select-one', value: boolean, file: AppFile): void
  (event: 'select-range', file: AppFile): void
  (event: 'context-menu', file: AppFile, event: MouseEvent): void
  (event: 'upload-many', files: FileList, dirId?: string): void
}>()

const gridEl = ref<HTMLElement | null>(null)

/**
 * Arrow keys walk the card navigation buttons, matching the table view's
 * keyboard model — Tab reaches the card's own controls (checkbox,
 * dropdown), arrows move between cards.
 */
const onGridKeydown = (event: KeyboardEvent) => {
  if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return

  const target = event.target as HTMLElement | null
  if (!target?.hasAttribute('data-row-nav')) return

  const buttons = [...(gridEl.value?.querySelectorAll<HTMLElement>('[data-row-nav]') ?? [])]
  const index = buttons.indexOf(target)
  const next = buttons[index + (event.key === 'ArrowDown' ? 1 : -1)]
  if (index < 0 || !next) return

  event.preventDefault()
  next.focus()
}
</script>

<template>
  <div
    ref="gridEl"
    class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-3 p-3 rounded-b-lg bg-white dark:bg-brownish-900"
    data-testid="files-grid"
    @keydown="onGridKeydown"
  >
    <FileGridCard
      v-for="file in props.items"
      :key="file.id"
      :file="file"
      :checkedIds="props.checkedIds"
      :hideDelete="props.hideDelete"
      :share="props.share"
      :hideCheckbox="props.hideCheckbox"
      :highlighted="props.searchedFileId === file.id"
      @actions="(f: AppFile) => emits('actions', f)"
      @deselect-all="emits('deselect-all')"
      @details="(f: AppFile) => emits('details', f)"
      @download="(f: AppFile) => emits('download', f)"
      @remove="(f: AppFile) => emits('remove', f)"
      @rename="(f: AppFile) => emits('rename', f)"
      @sharing="(f: AppFile) => emits('sharing', f)"
      @fork="(f: AppFile) => emits('fork', f)"
      @leave="(f: AppFile) => emits('leave', f)"
      @select-one="(v: boolean, f: AppFile) => emits('select-one', v, f)"
      @select-range="(f: AppFile) => emits('select-range', f)"
      @context-menu="(f: AppFile, e: MouseEvent) => emits('context-menu', f, e)"
      @upload-many="(f: FileList, d?: string) => emits('upload-many', f, d)"
    />
  </div>
</template>
