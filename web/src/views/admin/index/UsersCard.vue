<script setup lang="ts">
import CardBox from '@/components/ui/CardBox.vue'
import CardBoxComponentHeader from '@/components/ui/CardBoxComponentHeader.vue'
import SortableName from '@/components/ui/SortableName.vue'
import PuppyLoader from '@/components/ui/PuppyLoader.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseButtons from '@/components/ui/BaseButtons.vue'
import BaseIcon from '@/components/ui/BaseIcon.vue'
import TableCheckboxCell from '@/components/ui/TableCheckboxCell.vue'
import CardBoxModal from '@/components/ui/CardBoxModal.vue'
import UserRow from './UserRow.vue'
import { index } from '!/admin/users'
import { usersBulk } from '!/admin/bulk'
import { notification, errorNotification } from '!/notify'
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import type { Search, User, BulkAction } from 'types/admin/users'
import { mdiAccountCheck, mdiAccountOff, mdiDelete, mdiSearchWeb } from '@mdi/js'
import type { Paginated } from 'types'

const props = defineProps<{
  class?: string
}>()

const { t } = useI18n()

const paginated = ref<Paginated<User>>()
const query = ref<Search>({
  sort: 'created_at',
  order: 'desc',
  search: undefined,
  limit: 15,
  offset: 0
})

const search = ref('')

const total = computed(() => paginated.value?.total || 0)

const disablePreviousPage = computed(() => {
  if (!paginated.value) return true
  return !query.value.offset
})

const disableNextPage = computed(() => {
  if (!paginated.value) return true
  return (query.value.offset || 0) + (query.value.limit || 15) > paginated.value.total
})

const rangeStart = computed(() => (query.value.offset || 0) + 1)
const rangeEnd = computed(() => (query.value.offset || 0) + (paginated.value?.data.length || 0))

const previousPage = () => {
  if (!paginated.value) return
  if (!query.value.offset) return
  query.value.offset = query.value.offset - (query.value.limit || 15)
}

const nextPage = () => {
  if (!paginated.value) return
  if (typeof query.value.offset === 'undefined') query.value.offset = 0
  query.value.offset = query.value.offset + (query.value.limit || 15)
}

const find = async () => {
  paginated.value = await index(query.value)
}

watch(query, find, { deep: true, immediate: true })

// Selection lives as an array of user ids from the current page.
const selectedIds = ref<string[]>([])
const bulkRunning = ref(false)

const allSelected = computed(() => {
  if (!paginated.value || paginated.value.data.length === 0) return false
  return paginated.value.data.every((user) => selectedIds.value.includes(user.id))
})

const toggleAll = (value: boolean) => {
  if (!paginated.value) return
  selectedIds.value = value ? paginated.value.data.map((user) => user.id) : []
}

const toggleRow = (id: string, value: boolean) => {
  if (value) {
    if (!selectedIds.value.includes(id)) selectedIds.value = [...selectedIds.value, id]
  } else {
    selectedIds.value = selectedIds.value.filter((selected) => selected !== id)
  }
}

// Drop ids that left the page (pagination, deletion, search change).
watch(
  () => paginated.value?.data.map((user) => user.id).join(','),
  () => {
    if (!paginated.value) return
    const live = new Set(paginated.value.data.map((user) => user.id))
    selectedIds.value = selectedIds.value.filter((id) => live.has(id))
  }
)

// Bulk action bar + confirm modal state.
const pendingAction = ref<BulkAction>()
const confirmOpen = ref(false)

const actionLabel = computed(() => {
  switch (pendingAction.value) {
    case 'disable':
      return t('admin.users.actionDisable')
    case 'enable':
      return t('admin.users.actionEnable')
    case 'delete':
      return t('admin.users.actionDelete')
    default:
      return ''
  }
})

const openBulk = (action: BulkAction) => {
  if (selectedIds.value.length === 0 || bulkRunning.value) return
  pendingAction.value = action
  confirmOpen.value = true
}

const runBulk = async () => {
  if (!pendingAction.value || bulkRunning.value) return
  bulkRunning.value = true

  try {
    const action = pendingAction.value
    const response = await usersBulk(action, selectedIds.value)
    const failed = response.results.filter((result) => !result.ok)
    const succeeded = response.results.length - failed.length

    if (failed.length === 0) {
      notification(
        t('admin.users.bulkDone'),
        t('admin.users.bulkSummary', { success: succeeded, failed: 0 }),
        'success'
      )
    } else {
      const ids = failed.map((result) => `${result.id}: ${result.error}`).join(', ')
      notification(
        t('admin.users.bulkFailed'),
        `${t('admin.users.bulkSummary', { success: succeeded, failed: failed.length })} — ${ids}`,
        'error'
      )
    }

    selectedIds.value = []
    confirmOpen.value = false
    await find()
  } catch (error) {
    errorNotification(error)
  } finally {
    bulkRunning.value = false
  }
}
</script>
<template>
  <div class="flex" :class="props.class">
    <CardBox class="flex w-full">
      <CardBoxComponentHeader :title="$t('admin.users.title', { total })">
        <div class="flex flex-wrap items-center gap-2 px-4 py-3">
          <div class="relative">
            <input
              type="text"
              v-model="search"
              :placeholder="$t('admin.users.searchPlaceholder')"
              @keyup.enter="query.search = search"
              class="h-[34px] w-48 sm:w-60 pl-3 pr-8 text-sm rounded-lg transition duration-150 ease-in-out bg-white dark:bg-brownish-800 border border-paper-300 dark:border-brownish-600 text-brownish-900 dark:text-white placeholder-brownish-100/60 dark:placeholder-brownish-400 focus:outline-none focus:ring-2 focus:ring-offset-0 focus:ring-redish-400/60 dark:focus:ring-redish-500/50"
            />
            <button
              type="button"
              @click="query.search = search"
              class="absolute right-2 top-1/2 -translate-y-1/2 text-brownish-400 dark:text-brownish-50 hover:text-brownish-900 dark:hover:text-white transition-colors"
              :aria-label="$t('common.search')"
            >
              <BaseIcon :path="mdiSearchWeb" :size="15" />
            </button>
          </div>
        </div>
      </CardBoxComponentHeader>

      <!-- Bulk action bar: appears when rows are selected -->
      <div
        v-if="selectedIds.length > 0"
        class="flex flex-wrap items-center gap-3 px-4 py-3 border-b border-paper-200 dark:border-brownish-700/50 bg-paper-50 dark:bg-brownish-800/40"
      >
        <span class="text-sm font-medium">
          {{ $t('admin.users.selectedCount', { count: selectedIds.length }) }}
        </span>
        <BaseButtons>
          <BaseButton
            :icon="mdiAccountOff"
            :small="true"
            :label="$t('admin.users.bulkDisable')"
            :disabled="bulkRunning"
            @click="openBulk('disable')"
          />
          <BaseButton
            :icon="mdiAccountCheck"
            :small="true"
            :label="$t('admin.users.bulkEnable')"
            :disabled="bulkRunning"
            @click="openBulk('enable')"
          />
          <BaseButton
            :icon="mdiDelete"
            :small="true"
            color="danger"
            :label="$t('admin.users.bulkDelete')"
            :disabled="bulkRunning"
            @click="openBulk('delete')"
          />
        </BaseButtons>
      </div>

      <div class="overflow-x-auto -mx-4 -mb-4" v-if="paginated">
        <table class="w-full">
          <thead>
            <tr>
              <th class="lg:w-1">
                <TableCheckboxCell
                  :model-value="allSelected"
                  :label="$t('common.selectAll')"
                  @update:model-value="toggleAll"
                />
              </th>
              <th><SortableName :label="$t('common.email')" name="email" v-model="query" /></th>
              <th>{{ $t('admin.users.tfa') }}</th>
              <th>{{ $t('admin.role') }}</th>
              <th>{{ $t('admin.users.emailVerified') }}</th>
              <th><SortableName :label="$t('admin.users.joined')" name="created_at" v-model="query" /></th>
              <th>{{ $t('admin.users.lastActive') }}</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            <UserRow
              :user="user"
              :selected="selectedIds.includes(user.id)"
              @update:selected="toggleRow(user.id, $event)"
              v-for="user in paginated.data"
              :key="user.id"
            />
          </tbody>
        </table>

        <div class="flex items-center justify-between px-4 py-3 border-t border-paper-200 dark:border-brownish-700/50">
          <BaseButton :label="$t('admin.pagination.previous')" @click="previousPage" :disabled="disablePreviousPage" :small="true" />
          <span class="text-xs text-brownish-400 dark:text-brownish-50">{{ $t('admin.pagination.range', { start: rangeStart, end: rangeEnd, total }) }}</span>
          <BaseButton :label="$t('admin.pagination.next')" @click="nextPage" :disabled="disableNextPage" :small="true" />
        </div>
      </div>
      <PuppyLoader v-else />
    </CardBox>

    <!-- Bulk confirmation -->
    <CardBoxModal
      v-model="confirmOpen"
      :title="$t('admin.users.bulkConfirmTitle')"
      :button="pendingAction === 'delete' ? 'danger' : 'info'"
      :button-label="$t('common.confirm')"
      :has-cancel="true"
      :confirm-disabled="bulkRunning"
      @confirm="runBulk"
    >
      <p>
        {{ $t('admin.users.bulkConfirmText', { action: actionLabel, count: selectedIds.length }) }}
      </p>
    </CardBoxModal>
  </div>
</template>
