<script setup lang="ts">
import ActionsDropdown from '@/components/files/browser/ActionsDropdown.vue'
import FileThumbnail from '@/components/files/FileThumbnail.vue'
import TableCheckboxCell from '@/components/ui/TableCheckboxCell.vue'
import TruncatedSpan from '@/components/ui/TruncatedSpan.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseIcon from '@/components/ui/BaseIcon.vue'
import { formatPrettyDate, formatSize } from '!'
import {
  mdiDotsVertical,
  mdiAlertCircleOutline,
  mdiCloudSyncOutline,
  mdiFolderAccount,
  mdiShareVariantOutline,
  mdiUploadOutline
} from '@mdi/js'
import type { AppFile } from 'types'
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { store as uploadStore } from '!/storage/upload'
import { SHARED_WITH_ME_DIR_ID } from '!/storage'
import { prettyMime } from '@/utils/mime'
import { openDir, openFileOrDetails } from './fileNav'

/**
 * Grid-mode counterpart of `TableFileRow`. Every affordance of the table
 * row is present: single click selects, double click opens, the dropdown
 * and the cursor context menu expose the same action set, checkboxes and
 * drag-into-folder uploads behave identically — only the layout differs.
 */
const router = useRouter()
const { t } = useI18n()

const isDropZone = ref(false)

const props = defineProps<{
  file: AppFile
  checkedIds: Set<string>
  hideDelete?: boolean
  share?: boolean
  hideCheckbox?: boolean
  highlighted?: boolean
}>()

const emits = defineEmits<{
  (event: 'actions', file: AppFile): void
  (event: 'deselect-all'): void
  (event: 'details', file: AppFile): void
  (event: 'download', file: AppFile): void
  (event: 'sharing', file: AppFile): void
  (event: 'remove', file: AppFile): void
  (event: 'rename', file: AppFile): void
  (event: 'fork', file: AppFile): void
  (event: 'leave', file: AppFile): void
  (event: 'select-one', value: boolean, file: AppFile): void
  (event: 'select-range', file: AppFile): void
  (event: 'context-menu', file: AppFile, event: MouseEvent): void
  (event: 'upload-many', files: FileList, dirId?: string): void
}>()

const selectOne = (value: boolean) => {
  emits('select-one', value, props.file)
}

const checked = computed({
  get: () => props.checkedIds.has(props.file.id),
  set: (v) => selectOne(v)
})

const upload = uploadStore()

const isUnfinished = computed(() => {
  return props.file.mime !== 'dir' && !props.file.finished_upload_at
})

const showProgress = computed(() => {
  if (!isUnfinished.value) {
    return false
  }

  return (
    upload.running.some((item) => item.id === props.file.id) ||
    upload.waiting.some((item) => item.id === props.file.id)
  )
})

const isStalledUpload = computed(() => isUnfinished.value && !showProgress.value)

const isDir = computed(() => {
  return props.file.mime === 'dir'
})

const fileName = computed(() => {
  const name = props.file.name || '...'

  return props.file.mime === 'dir' ? `${name}/` : name
})

const fileSize = computed(() => {
  return props.file.size ? formatSize(props.file.size) : ''
})

const fileModifiedAt = computed(() => {
  return props.file.file_modified_at ? formatPrettyDate(props.file.file_modified_at) : ''
})

/**
 * Owner-of-row email surfaced under the file name, mirroring the badge in
 * the table row for incoming shares.
 */
const ownerBadgeEmail = computed(() => {
  if (props.file.is_owner) return ''
  return props.file.owner_email || props.file.shared_by_email || ''
})

const isSharedOut = computed(() => {
  if (!props.file.is_owner) return false
  return (props.file.shared_with_count ?? 0) > 0
})

/**
 * The synthetic "Shared with me" root is a navigation affordance only —
 * no checkbox, no dropdown, no context menu, just the click-through.
 */
const isSyntheticRoot = computed(() => props.file.id === SHARED_WITH_ME_DIR_ID)

const click = (event: MouseEvent) => {
  if (event.shiftKey) {
    emits('select-range', props.file)
    return
  }
  singleClick()
}

const doubleClick = () => {
  if (isSyntheticRoot.value) return
  if (isDir.value) {
    openDir(router, props.file)
  } else {
    openFileOrDetails(router, props.file, (file) => emits('details', file))
  }
}

const singleClick = () => {
  if (isSyntheticRoot.value) return
  emits('deselect-all')
  selectOne(!checked.value)
}

const emitContextMenu = (event: MouseEvent) => {
  if (isSyntheticRoot.value || !props.file.id) return
  emits('context-menu', props.file, event)
}

const dragend = (e: DragEvent) => {
  if (props.file.mime !== 'dir') {
    return
  }

  isDropZone.value = false

  e.preventDefault()
  e.stopPropagation()
}

const dragover = (e: DragEvent) => {
  if (props.file.mime !== 'dir') {
    return
  }

  isDropZone.value = true

  e.preventDefault()
  e.stopPropagation()
}

const drop = (e: DragEvent) => {
  if (props.file.mime !== 'dir') {
    return
  }

  isDropZone.value = false

  e.preventDefault()
  e.stopPropagation()

  if (e.dataTransfer?.files && e.dataTransfer.files.length) {
    emits('upload-many', e.dataTransfer.files, props.file.id)

    setTimeout(() => {
      openDir(router, props.file)
    }, 100)
  }
}
</script>

<template>
  <div
    class="relative flex flex-col items-center gap-2 p-3 rounded-lg border text-center cursor-pointer prevent-select"
    :class="{
      'bg-paper-50 dark:bg-brownish-700 !border-redish-300': !!checked,
      'outline-2 outline-redish-300 outline': isDropZone,
      '!border-greeny-300 dark:!border-greeny-700': props.highlighted
    }"
    data-testid="file-card"
    :data-file-id="file.id"
    :data-file-name="file.name"
    @contextmenu.prevent="emitContextMenu"
    @dragenter="dragover"
    @dragleave="dragend"
    @dragend="dragend"
    @dragover="dragover"
    @drop="drop"
  >
    <div class="w-full flex items-start justify-between min-h-[1.75rem]">
      <span v-if="!props.hideCheckbox && !isSyntheticRoot" class="inline-flex" @click.stop>
        <TableCheckboxCell v-model="checked" :label="$t('common.selectRow', { name: file.name })" />
      </span>
      <span v-else />

      <span v-if="isSharedOut" class="inline-flex items-center text-brownish-400 dark:text-brownish-50" :title="t('files.row.sharedWith', props.file.shared_with_count ?? 0)">
        <BaseIcon :path="mdiShareVariantOutline" :size="14" />
      </span>
      <BaseIcon
        v-else-if="file.id === SHARED_WITH_ME_DIR_ID"
        :path="mdiFolderAccount"
        :size="16"
        class="text-orangy-800 dark:text-orangy-400"
        data-testid="shared-with-me-folder-icon"
      />

      <template v-if="!isSyntheticRoot">
        <BaseButton
          class="sm:hidden"
          color="dark"
          :icon="mdiDotsVertical"
          small
          :title="$t('common.actions')"
          :aria-label="$t('common.actions')"
          aria-haspopup="menu"
          @click.stop="emits('actions', file)"
          :disabled="!props.file.id"
        />
        <span class="hidden sm:block" @click.stop>
          <ActionsDropdown
            :model-value="props.file"
            :disabled="!props.file.id"
            :hide-delete="props.hideDelete"
            :share="props.share"
            @details="(f: AppFile) => emits('details', f)"
            @download="(f: AppFile) => emits('download', f)"
            @remove="(f: AppFile) => emits('remove', f)"
            @rename="(f: AppFile) => emits('rename', f)"
            @sharing="(f: AppFile) => emits('sharing', f)"
            @fork="(f: AppFile) => emits('fork', f)"
            @leave="(f: AppFile) => emits('leave', f)"
          />
        </span>
      </template>
    </div>

    <button
      class="flex flex-col items-center gap-2 w-full cursor-pointer bg-transparent border-0 p-0 focus:outline-none focus:ring-2 focus:ring-inset focus:ring-redish-400/60 dark:focus:ring-redish-500/50 rounded-lg"
      data-row-nav
      @click="click"
      @dblclick="doubleClick"
      @keydown.space.prevent="singleClick"
    >
      <FileThumbnail :file="file" img-class="w-16 h-16 rounded-lg">
        <BaseIcon
          v-if="showProgress"
          :path="mdiUploadOutline"
          :size="28"
          class="text-greeny-500 dark:text-greeny-300"
          data-testid="uploading-icon"
        />
        <BaseIcon
          v-else-if="isStalledUpload"
          :path="mdiAlertCircleOutline"
          :size="28"
          class="text-orangy-800 dark:text-orangy-400"
          :title="$t('files.row.uploadIncomplete')"
          data-testid="stalled-upload-icon"
        />
      </FileThumbnail>

      <span class="w-full max-w-full text-sm font-medium text-brownish-700 dark:text-brownish-50">
        <TruncatedSpan :text="fileName" />
      </span>
    </button>

    <span
      v-if="ownerBadgeEmail"
      class="inline-block max-w-full truncate px-2 py-0.5 rounded-full text-xs font-semibold bg-paper-100 dark:bg-brownish-800 text-brownish-700 dark:text-brownish-50"
      :title="$t('files.row.ownedBy', { email: ownerBadgeEmail })"
      data-testid="shared-by-badge"
    >
      {{ ownerBadgeEmail }}
    </span>

    <div class="w-full text-xs text-brownish-400 dark:text-brownish-50 flex flex-col">
      <span :title="fileSize">{{ fileSize || prettyMime(props.file.mime) }}</span>
      <span :title="fileModifiedAt">{{ fileModifiedAt }}</span>
    </div>

    <span
      v-if="props.file.pending_version != null"
      class="absolute top-2 right-2 inline-flex items-center text-orangy-800 dark:text-orangy-400"
      :title="$t('files.row.savingInAnotherSession')"
    >
      <BaseIcon :path="mdiCloudSyncOutline" :size="14" />
    </span>
  </div>

  <div
    v-if="showProgress"
    class="border-b-2 border-greeny-500 dark:border-greeny-400 rounded-full"
    :style="{
      width: Math.min(
        100,
        props.file.finished_upload_at
          ? 100
          : props.file.chunks
            ? Math.ceil(((props.file.chunks_stored || 0) / props.file.chunks) * 100)
            : 0
      ) + '%'
    }"
  ></div>
</template>

<style lang="css">
.prevent-select {
  -webkit-user-select: none;
  -ms-user-select: none;
  user-select: none;
}
</style>
