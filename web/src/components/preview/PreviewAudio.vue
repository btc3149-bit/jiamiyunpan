<script setup lang="ts">
import { ref, watch, onUnmounted } from 'vue'
import type { Preview } from '!/preview'
import SpinnerIcon from '../ui/SpinnerIcon.vue'
import * as logger from '!/logger'

const props = defineProps<{
  modelValue: Preview
}>()

const blobUrl = ref<string>()
const progress = ref(0)
const error = ref<string>()

let abortController: AbortController | null = null

/**
 * Same lifecycle contract as the video preview: one object URL per open
 * preview, revoked the moment the component unmounts or the watched
 * preview swaps, so decrypted audio never lingers in the blob registry.
 */
function cleanup() {
  abortController?.abort()
  abortController = null
  if (blobUrl.value) {
    URL.revokeObjectURL(blobUrl.value)
    blobUrl.value = undefined
  }
}

async function load() {
  cleanup()
  progress.value = 0
  error.value = undefined

  abortController = new AbortController()
  const signal = abortController.signal
  const preview = props.modelValue

  logger.debug(`[audio-preview] opening "${preview.name}" — mime=${preview.mime}`)

  try {
    const data = await preview.load((bytes) => {
      if (preview.size && !signal.aborted) {
        progress.value = Math.min(100, Math.round((bytes / preview.size) * 100))
      }
    })
    if (signal.aborted) return
    blobUrl.value = URL.createObjectURL(new Blob([data], { type: preview.mime }))
    progress.value = 100
  } catch (e) {
    if (signal.aborted) return
    error.value = e instanceof Error ? e.message : 'Failed to load audio'
    logger.error('[audio-preview] preview.load() failed', e)
  }
}

watch(() => props.modelValue, load, { immediate: true })
onUnmounted(cleanup)
</script>

<template>
  <div v-if="!blobUrl && !error" class="flex flex-col items-center justify-center w-full h-full gap-4">
    <SpinnerIcon />
    <div class="text-sm text-brownish-50">{{ $t('preview.audio.loading', { progress }) }}</div>
    <div class="w-64 h-2 bg-brownish-700 rounded-full overflow-hidden">
      <div class="h-full bg-greeny-400 transition-all" :style="{ width: `${progress}%` }" />
    </div>
  </div>

  <div v-else-if="error" data-testid="audio-preview-error" class="flex flex-col items-center justify-center w-full h-full gap-2 text-redish-100">
    <span>{{ $t('preview.audio.loadError', { error }) }}</span>
  </div>

  <div v-else class="flex flex-col items-center justify-center w-full h-full">
    <audio :src="blobUrl" controls data-testid="audio-preview-player" class="w-full max-w-xl" />
  </div>
</template>
