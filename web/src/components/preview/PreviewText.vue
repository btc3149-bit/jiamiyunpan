<script setup lang="ts">
import { ref, watch, onUnmounted } from 'vue'
import type { Preview } from '!/preview'
import { decodeTextPreview, TEXT_PREVIEW_MAX_BYTES } from '!/preview'
import SpinnerIcon from '../ui/SpinnerIcon.vue'
import * as logger from '!/logger'

const props = defineProps<{
  modelValue: Preview
}>()

const content = ref('')
const truncated = ref(false)
const loading = ref(false)
const error = ref<string>()

let abortController: AbortController | null = null

/**
 * Everything rendered below goes through `{{ }}` interpolation, which HTML-
 * escapes by construction — an uploaded "file.html" shows its source, it
 * never executes. There is deliberately no v-html anywhere in this
 * component.
 */

function cleanup() {
  abortController?.abort()
  abortController = null
}

async function load() {
  cleanup()
  loading.value = true
  truncated.value = false
  error.value = undefined
  content.value = ''

  abortController = new AbortController()
  const preview = props.modelValue

  logger.debug(`[text-preview] opening "${preview.name}" — mime=${preview.mime} size=${preview.size}`)

  try {
    const head = await fetchHead(preview)
    const decoded = decodeTextPreview(head.data)
    content.value = decoded.text
    // The chunked head-stop can land exactly on the byte cap while more
    // file exists — the stored size settles what decodeTextPreview can't
    // see. Only the head path uses it; a full load already carries the
    // whole truth in its byte count.
    truncated.value =
      decoded.truncated || (head.headOnly && preview.size !== undefined && preview.size > head.data.length)
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to load file'
    logger.error('[text-preview] load failed', e)
  } finally {
    loading.value = false
  }
}

/**
 * Pull just enough plaintext for the preview: the head of oversized files
 * is assembled chunk-by-chunk so a huge log doesn't decrypt in full, while
 * everything at or under the cap — and previews without per-chunk access —
 * takes the regular whole-file path.
 */
async function fetchHead(preview: Preview): Promise<{ data: Uint8Array; headOnly: boolean }> {
  const totalChunks = preview.chunks
  const size = preview.size

  if (totalChunks && size !== undefined && size > TEXT_PREVIEW_MAX_BYTES) {
    const parts: Uint8Array[] = []
    let accumulated = 0

    for (let i = 0; i < totalChunks && accumulated <= TEXT_PREVIEW_MAX_BYTES; i++) {
      const chunk = await preview.loadChunk(i, abortController?.signal)
      parts.push(chunk)
      accumulated += chunk.length
    }

    const combined = new Uint8Array(accumulated)
    let offset = 0
    for (const part of parts) {
      combined.set(part, offset)
      offset += part.length
    }
    return { data: combined, headOnly: true }
  }

  return { data: await preview.load(), headOnly: false }
}

watch(() => props.modelValue, load, { immediate: true })
onUnmounted(cleanup)
</script>

<template>
  <div v-if="loading" class="flex flex-col items-center justify-center w-full h-full gap-4">
    <SpinnerIcon />
  </div>

  <div v-else-if="error" class="flex flex-col items-center justify-center w-full h-full gap-2 text-redish-100">
    <span>{{ error }}</span>
  </div>

  <div v-else class="flex flex-col w-full max-w-5xl h-full mx-auto px-4">
    <div v-if="truncated" data-testid="text-preview-truncated" class="text-center text-xs text-brownish-400 dark:text-brownish-100 py-2 flex-shrink-0">
      {{ $t('preview.text.truncated') }}
    </div>
    <pre
      data-testid="text-preview-content"
      class="text-preview-body flex-1 overflow-auto m-0 p-4 rounded-lg whitespace-pre-wrap break-words"
    >{{ content }}</pre>
  </div>
</template>

<style scoped>
.text-preview-body {
  background: rgba(0, 0, 0, 0.35);
  color: inherit;
  font-family: 'SF Mono', 'Fira Code', 'JetBrains Mono', 'Cascadia Code', monospace;
  font-size: 0.8125rem;
  line-height: 1.6;
  tab-size: 2;
}
</style>
