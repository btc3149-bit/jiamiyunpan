<script setup lang="ts">
import type { AppFile } from 'types'
import ActionsButtons from './ActionsButtons.vue'
import { ref, nextTick, onMounted, onUnmounted } from 'vue'
import { vOnClickOutside } from '@vueuse/components'

/**
 * Cursor-anchored context menu for file rows and grid cards. It reuses
 * `ActionsButtons`, so the action set and every gating rule (synthetic
 * root, mid-upload rows, share roles) is byte-for-byte the same as the
 * existing row dropdown — only the trigger differs.
 *
 * The panel teleports to `body` with `position: fixed`, so rows inside
 * scroll containers or `overflow: hidden` cards can never clip it.
 */
const props = defineProps<{
  hideDelete?: boolean
  share?: boolean
}>()

const emits = defineEmits<{
  (event: 'remove', file: AppFile): void
  (event: 'details', file: AppFile): void
  (event: 'rename', file: AppFile): void
  (event: 'sharing', file: AppFile): void
  (event: 'download', file: AppFile): void
  (event: 'fork', file: AppFile): void
  (event: 'leave', file: AppFile): void
}>()

const menuEl = ref<HTMLElement | null>(null)
const visible = ref(false)
const activeFile = ref<AppFile | null>(null)
const position = ref({ left: '0px', top: '0px' })

const close = () => {
  visible.value = false
}

/**
 * Open at the pointer. Measuring after the panel renders (menu content
 * depends on the file's capabilities) keeps the viewport flip accurate;
 * an 8px margin stops it hugging the screen edge when opened in a corner.
 */
const open = async (event: MouseEvent, file: AppFile) => {
  const left = event.clientX
  const top = event.clientY

  activeFile.value = file
  visible.value = true

  await nextTick()

  let x = left
  let y = top
  const el = menuEl.value
  if (el) {
    const width = el.offsetWidth
    const height = el.offsetHeight
    if (x + width > window.innerWidth) {
      x = Math.max(8, window.innerWidth - width - 8)
    }
    if (y + height > window.innerHeight) {
      y = Math.max(8, window.innerHeight - height - 8)
    }
  }

  position.value = { left: `${x}px`, top: `${y}px` }
}

/**
 * Any scroll — the page or an inner container (capture phase) — moves the
 * point the menu was anchored to, so the panel closes instead of drifting
 * away from its row. Window resizes behave the same.
 */
const onScroll = () => {
  if (visible.value) close()
}

const onKeydown = (event: KeyboardEvent) => {
  if (event.key === 'Escape' && visible.value) {
    close()
  }
}

onMounted(() => {
  window.addEventListener('keydown', onKeydown)
  window.addEventListener('scroll', onScroll, true)
  window.addEventListener('resize', onScroll)
})

onUnmounted(() => {
  window.removeEventListener('keydown', onKeydown)
  window.removeEventListener('scroll', onScroll, true)
  window.removeEventListener('resize', onScroll)
})

defineExpose({ open, close })
</script>

<template>
  <Teleport to="body">
    <div
      v-if="visible && activeFile"
      ref="menuEl"
      v-on-click-outside="close"
      role="menu"
      :aria-label="$t('common.actions')"
      data-testid="file-context-menu"
      class="bg-white dark:bg-brownish-700 border border-paper-300 dark:border-brownish-600 rounded-md shadow-lg max-h-modal w-max min-w-[10rem] max-w-[16rem] z-[60] py-2"
      :style="position"
      style="position: fixed"
    >
      <ActionsButtons
        :model-value="activeFile"
        :hide-delete="props.hideDelete"
        :share="props.share"
        @remove="(f: AppFile) => { close(); emits('remove', f) }"
        @details="(f: AppFile) => { close(); emits('details', f) }"
        @download="(f: AppFile) => { close(); emits('download', f) }"
        @rename="(f: AppFile) => { close(); emits('rename', f) }"
        @sharing="(f: AppFile) => { close(); emits('sharing', f) }"
        @fork="(f: AppFile) => { close(); emits('fork', f) }"
        @leave="(f: AppFile) => { close(); emits('leave', f) }"
      />
    </div>
  </Teleport>
</template>
