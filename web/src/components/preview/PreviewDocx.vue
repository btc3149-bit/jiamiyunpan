<script setup lang="ts">
import { ref, watch, onUnmounted } from 'vue'
import type { Preview } from '!/preview'
import DOMPurify from 'dompurify'
import SpinnerIcon from '../ui/SpinnerIcon.vue'
import * as logger from '!/logger'

const props = defineProps<{
  modelValue: Preview
}>()

const html = ref('')
const loading = ref(false)
const error = ref<string>()

let abortController: AbortController | null = null

/**
 * mammoth emits relatively clean semantic HTML, but its input is a user
 * upload, so nothing it produced is trusted: DOMPurify strips scripts,
 * event handlers and javascript: URLs before the fragment ever reaches
 * v-html.
 */
function sanitize(fragment: string): string {
  return DOMPurify.sanitize(fragment, {
    USE_PROFILES: { html: true },
    FORBID_TAGS: ['style', 'form', 'input'],
    FORBID_ATTR: ['style']
  })
}

function cleanup() {
  abortController?.abort()
  abortController = null
}

async function load() {
  cleanup()
  loading.value = true
  error.value = undefined
  html.value = ''

  abortController = new AbortController()
  const preview = props.modelValue

  logger.debug(`[docx-preview] opening "${preview.name}" — mime=${preview.mime}`)

  try {
    const data = await preview.load()
    if (abortController.signal.aborted) return

    // Imported lazily so the (sizable) docx parser stays out of the
    // initial bundle and only loads when a Word document is previewed.
    const { convertToHtml } = await import('mammoth')
    if (abortController.signal.aborted) return

    const result = await convertToHtml({ buffer: data })
    if (abortController.signal.aborted) return

    html.value = sanitize(result.value)
  } catch (e) {
    if (abortController.signal.aborted) return
    error.value = e instanceof Error ? e.message : 'Failed to load document'
    logger.error('[docx-preview] load failed', e)
  } finally {
    loading.value = false
  }
}

watch(() => props.modelValue, load, { immediate: true })
onUnmounted(cleanup)
</script>

<template>
  <div v-if="loading" class="flex flex-col items-center justify-center w-full h-full gap-4">
    <SpinnerIcon />
  </div>

  <div v-else-if="error" data-testid="docx-preview-error" class="flex flex-col items-center justify-center w-full h-full gap-2 text-redish-100">
    <span>{{ $t('preview.docx.loadError', { error }) }}</span>
  </div>

  <!-- Light canvas instead of the viewer's near-black: documents are meant
       to be read like paper, and mammoth output assumes white background. -->
  <div v-else class="docx-preview-canvas flex-1 w-full h-full overflow-auto py-6">
    <!-- eslint-disable-next-line vue/no-v-html — content is DOMPurify-sanitized above -->
    <div class="docx-preview-page mx-auto bg-white text-gray-900 p-10 rounded shadow-lg" v-html="html" />
  </div>
</template>

<style scoped>
.docx-preview-canvas {
  background: #e7e5e4;
}

.docx-preview-page {
  max-width: 52rem;
  min-height: 100%;
  font-family: Georgia, 'Times New Roman', serif;
  line-height: 1.7;
}

.docx-preview-page :deep(h1) { font-size: 1.75rem; font-weight: 700; margin: 1.25rem 0 0.75rem; }
.docx-preview-page :deep(h2) { font-size: 1.4rem; font-weight: 700; margin: 1.1rem 0 0.6rem; }
.docx-preview-page :deep(h3) { font-size: 1.15rem; font-weight: 600; margin: 1rem 0 0.5rem; }
.docx-preview-page :deep(p) { margin: 0.6rem 0; }
.docx-preview-page :deep(ul),
.docx-preview-page :deep(ol) { margin: 0.6rem 0; padding-left: 1.5rem; }
.docx-preview-page :deep(img) { max-width: 100%; height: auto; }
.docx-preview-page :deep(table) { border-collapse: collapse; margin: 0.75rem 0; }
.docx-preview-page :deep(td),
.docx-preview-page :deep(th) { border: 1px solid #d6d3d1; padding: 0.35rem 0.6rem; }
.docx-preview-page :deep(a) { color: #9a3412; text-decoration: underline; }
</style>
