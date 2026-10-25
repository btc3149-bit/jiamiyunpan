<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { mdiRestore, mdiTrashCanOutline, mdiDeleteForeverOutline } from '@mdi/js'
import LayoutAuthenticatedWithLoader from '@/layouts/LayoutAuthenticatedWithLoader.vue'
import SectionMain from '@/components/ui/SectionMain.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseButtons from '@/components/ui/BaseButtons.vue'
import BaseIcon from '@/components/ui/BaseIcon.vue'
import CardBoxModal from '@/components/ui/CardBoxModal.vue'
import TruncatedSpan from '@/components/ui/TruncatedSpan.vue'
import { formatPrettyDate, formatSize } from '!/index'
import { notification, errorNotification } from '!/notify'
import { store as cryptoStore } from '!/crypto'
import * as trashApi from '!/storage/trash'

import type { AppFile } from 'types'

const { t } = useI18n()
const Crypto = cryptoStore()

interface TrashRow {
  id: string
  name: string
  mime: string
  size?: number
  deleted_at: number
  path: string
  key?: Uint8Array
}

const rows = ref<TrashRow[]>([])
const total = ref(0)
const offset = ref(0)
const LIMIT = 50

const loading = ref(false)
const error = ref(false)

const isDir = (row: TrashRow) => row.mime === 'dir'

async function load(append = false): Promise<void> {
  if (loading.value) return
  if (!Crypto.keypair?.input) return

  loading.value = true
  error.value = false

  try {
    if (!append) {
      offset.value = 0
    }

    const page = await trashApi.list(LIMIT, offset.value)
    total.value = page.total

    const next: TrashRow[] = []
    for (const item of page.items) {
      const { file, path } = await trashApi.decryptItem(item, Crypto.keypair.input)
      next.push({
        id: file.id,
        name: file.name,
        mime: file.mime,
        size: file.size ?? undefined,
        deleted_at: item.deleted_at,
        path,
        key: file.key
      })
    }

    rows.value = append ? [...rows.value, ...next] : next
  } catch (e) {
    error.value = true
  } finally {
    loading.value = false
  }
}

watch(
  () => Crypto.keypair?.input,
  (v) => {
    if (v) load()
  },
  { immediate: true }
)

const hasMore = computed(() => rows.value.length < total.value)

const loadMore = () => {
  offset.value += LIMIT
  load(true)
}

/**
 * Restore. The server detects a name collision (via the keyed name_hash)
 * but cannot compute the " (restored)" suffix over ciphertext — a 409 is
 * answered by re-encrypting the suffixed name here and retrying once.
 */
const restoring = ref<string | null>(null)

const restore = async (row: TrashRow) => {
  if (restoring.value) return
  restoring.value = row.id

  try {
    try {
      await trashApi.restore(row.id)
    } catch (err: any) {
      const message = typeof err === 'object' ? (err?.message ?? String(err)) : String(err)
      if (!message.includes('trash_restore_name_conflict')) throw err

      if (!row.key || !Crypto.keypair) throw err

      const payload = await trashApi.buildRestoreRenamePayload(
        `${row.name} (restored)`,
        row.key,
        Crypto.keypair
      )
      await trashApi.restore(row.id, payload)
      notification(
        t('files.trash.restoredTitle'),
        t('files.trash.restoredRenamed', { name: row.name }),
        'success'
      )
    }

    if (restoring.value === row.id) {
      notification(
        t('files.trash.restoredTitle'),
        t('files.trash.restoredText', { name: row.name }),
        'success'
      )
    }
  } catch (err) {
    errorNotification(err)
  } finally {
    restoring.value = null
    await load()
  }
}

/**
 * Permanent delete keeps the file browser's confirmation grammar: a folder
 * — which takes everything under it — asks for its name to be typed; a
 * single file is one named row and needs only the confirm button.
 */
const purgeTarget = ref<TrashRow | null>(null)
const purgeTyped = ref('')
const purgeBusy = ref(false)

const purgeNeedsTyping = computed(() => !!purgeTarget.value && isDir(purgeTarget.value))
const purgeDisabled = computed(
  () => purgeNeedsTyping.value && purgeTyped.value.trim() !== purgeTarget.value?.name
)

const openPurge = (row: TrashRow) => {
  purgeTarget.value = row
  purgeTyped.value = ''
}

watch(purgeTarget, () => (purgeTyped.value = ''))

const confirmPurge = async () => {
  const row = purgeTarget.value
  if (!row || purgeDisabled.value || purgeBusy.value) return

  purgeBusy.value = true
  try {
    await trashApi.purge(row.id)
    notification(
      t('files.trash.purgedTitle'),
      t('files.trash.purgedText', { name: row.name }),
      'success'
    )
    purgeTarget.value = null
  } catch (err) {
    errorNotification(err)
  } finally {
    purgeBusy.value = false
    await load()
  }
}

/**
 * Empty trash — one confirm on top of the button, then every entry the
 * caller owns is gone for good.
 */
const emptyOpen = ref(false)
const emptyBusy = ref(false)

const confirmEmpty = async () => {
  if (emptyBusy.value) return

  emptyBusy.value = true
  try {
    await trashApi.empty()
    notification(t('files.trash.emptiedTitle'), t('files.trash.emptiedText'), 'success')
    emptyOpen.value = false
  } catch (err) {
    errorNotification(err)
  } finally {
    emptyBusy.value = false
    await load()
  }
}
</script>

<template>
  <LayoutAuthenticatedWithLoader v-slot="{ authenticated }">
    <SectionMain v-if="authenticated">
      <div class="flex items-center justify-between mb-4 gap-4">
        <h2 class="text-xl font-bold flex items-center gap-2">
          <BaseIcon :path="mdiTrashCanOutline" />
          {{ $t('files.trash.title') }}
        </h2>

        <BaseButton
          color="danger"
          :label="$t('files.trash.emptyTrash')"
          :disabled="!rows.length"
          data-testid="trash-empty"
          @click="emptyOpen = true"
        />
      </div>

      <div
        class="bg-white dark:bg-brownish-900 rounded-lg border border-paper-300/40 dark:border-brownish-700/40"
      >
        <div
          class="w-full flex rounded-t-lg bg-paper-100 dark:bg-brownish-950 border-b border-paper-300 dark:border-brownish-700/40 text-sm"
        >
          <div class="flex-1 p-2 pt-3 min-w-0">
            {{ $t('common.name') }}
          </div>
          <div class="hidden p-2 pt-3 sm:block w-56 shrink-0 truncate">
            {{ $t('files.trash.originalPath') }}
          </div>
          <div class="hidden p-2 pt-3 md:block w-24 shrink-0">
            {{ $t('common.size') }}
          </div>
          <div class="hidden p-2 pt-3 sm:block w-44 shrink-0">
            {{ $t('files.trash.deletedAt') }}
          </div>
          <div class="w-36 p-2 shrink-0"></div>
        </div>

        <div
          v-if="error"
          class="w-full py-10 flex flex-col items-center gap-3"
          data-testid="trash-error"
        >
          <span class="text-sm text-brownish-300 dark:text-brownish-50">
            {{ $t('errors.listFailed') }}
          </span>
          <BaseButton color="info" small :label="$t('common.retry')" @click="load()" />
        </div>

        <div
          v-else-if="!loading && !rows.length"
          class="w-full py-14 flex flex-col items-center gap-1"
          data-testid="trash-empty-state"
        >
          <BaseIcon :path="mdiTrashCanOutline" class="text-brownish-200" />
          <span class="text-brownish-300 dark:text-brownish-50">
            {{ $t('files.trash.empty') }}
          </span>
          <span class="text-xs text-brownish-200 dark:text-brownish-50">
            {{ $t('files.trash.emptyHint') }}
          </span>
        </div>

        <div v-else class="flex flex-col rounded-b-lg">
          <div
            v-for="row in rows"
            :key="row.id"
            class="w-full flex file-row-separator items-start"
            :data-testid="`trash-row-${row.id}`"
          >
            <div class="flex-1 p-2 pt-3 min-w-0 flex">
              <TruncatedSpan :text="isDir(row) ? `${row.name}/` : row.name" />
            </div>
            <div class="hidden p-2 pt-3 sm:block w-56 shrink-0 text-sm text-brownish-400">
              <TruncatedSpan :text="row.path || $t('files.trash.rootPath')" />
            </div>
            <div class="hidden p-2 pt-3 md:block w-24 shrink-0 text-sm">
              {{ row.size ? formatSize(row.size) : '' }}
            </div>
            <div class="hidden p-2 pt-3 sm:block w-44 shrink-0 text-sm">
              {{ formatPrettyDate(row.deleted_at) }}
            </div>
            <div class="w-36 p-2 shrink-0">
              <BaseButtons>
                <BaseButton
                  color="info"
                  :xs="true"
                  :icon="mdiRestore"
                  :label="$t('files.trash.restore')"
                  :disabled="restoring === row.id"
                  :data-testid="`trash-restore-${row.id}`"
                  @click="restore(row)"
                />
                <BaseButton
                  color="danger"
                  :xs="true"
                  :icon="mdiDeleteForeverOutline"
                  :title="$t('files.trash.purge')"
                  :data-testid="`trash-purge-${row.id}`"
                  @click="openPurge(row)"
                />
              </BaseButtons>
            </div>
          </div>

          <div v-if="loading" class="p-4 text-sm text-brownish-300" role="status">
            {{ $t('common.loading') }}
          </div>
        </div>

        <div v-if="hasMore && !loading" class="p-4 flex justify-center">
          <BaseButton
            color="light"
            small
            :label="$t('files.trash.loadMore')"
            data-testid="trash-load-more"
            @click="loadMore"
          />
        </div>
      </div>

      <!-- Permanent delete of one entry -->
      <CardBoxModal
        :title="$t('files.trash.purgeTitle')"
        button="danger"
        :model-value="!!purgeTarget"
        :button-label="$t('files.trash.purgeConfirmLabel')"
        :has-cancel="true"
        :confirm-disabled="purgeDisabled"
        @cancel="purgeTarget = null"
        @confirm="confirmPurge"
      >
        <template v-if="purgeTarget">
          <p>
            {{
              isDir(purgeTarget)
                ? $t('files.trash.purgeConfirmDirectory', { name: purgeTarget.name })
                : $t('files.trash.purgeConfirmFile', { name: purgeTarget.name })
            }}
          </p>

          <label v-if="purgeNeedsTyping" class="block mt-4 text-sm">
            <span class="block mb-1.5">
              {{ $t('files.trash.typeName', { name: purgeTarget.name }) }}
            </span>
            <input
              v-model="purgeTyped"
              type="text"
              autocomplete="off"
              data-testid="trash-purge-confirm-name"
              class="w-full bg-white dark:bg-brownish-800 border border-paper-300 dark:border-brownish-700 rounded-lg px-4 py-2 focus:outline-none focus:ring-2 focus:ring-offset-0 focus:ring-redish-400/60 dark:focus:ring-redish-500/50"
            />
          </label>
        </template>
      </CardBoxModal>

      <!-- Empty the whole trash -->
      <CardBoxModal
        :title="$t('files.trash.emptyTrashTitle')"
        button="danger"
        :model-value="emptyOpen"
        :button-label="$t('files.trash.emptyTrashConfirm')"
        :has-cancel="true"
        @cancel="emptyOpen = false"
        @confirm="confirmEmpty"
      >
        <p>{{ $t('files.trash.emptyTrashWarning', { count: rows.length }) }}</p>
      </CardBoxModal>
    </SectionMain>
  </LayoutAuthenticatedWithLoader>
</template>
