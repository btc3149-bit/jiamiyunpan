<script setup lang="ts">
import CardBox from '@/components/ui/CardBox.vue'
import CardBoxComponentHeader from '@/components/ui/CardBoxComponentHeader.vue'
import PuppyLoader from '@/components/ui/PuppyLoader.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseIcon from '@/components/ui/BaseIcon.vue'
import LayoutAdminWithLoader from '@/layouts/LayoutAdminWithLoader.vue'
import SectionMain from '@/components/ui/SectionMain.vue'
import { index } from '!/admin/audit'
import { formatPrettyDate } from '!/index'
import type { AuditLog, Search } from 'types/admin/audit'
import type { Paginated } from 'types'
import { mdiSearchWeb } from '@mdi/js'
import { computed, ref, watch } from 'vue'

const paginated = ref<Paginated<AuditLog>>()
const query = ref<Search>({
  limit: 15,
  offset: 0
})

const action = ref('')

const rows = computed(() => paginated.value?.data || [])
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
const rangeEnd = computed(() => (query.value.offset || 0) + rows.value.length)

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
  paginated.value = await index({ ...query.value })
}

const applyActionFilter = () => {
  query.value = { ...query.value, action: action.value || undefined, offset: 0 }
}

watch(query, find, { deep: true, immediate: true })
</script>
<template>
  <LayoutAdminWithLoader>
    <Suspense>
      <SectionMain>
        <CardBox>
          <CardBoxComponentHeader :title="$t('admin.audit.title', { total })">
            <div class="flex flex-wrap items-center gap-2 px-4 py-3">
              <div class="relative">
                <input
                  type="text"
                  v-model="action"
                  :placeholder="$t('admin.audit.actionFilterPlaceholder')"
                  @keyup.enter="applyActionFilter"
                  class="h-[34px] w-44 sm:w-56 pl-3 pr-8 text-sm rounded-lg transition duration-150 ease-in-out bg-white dark:bg-brownish-800 border border-paper-300 dark:border-brownish-600 text-brownish-900 dark:text-white placeholder-brownish-100/60 dark:placeholder-brownish-400 focus:outline-none focus:ring-2 focus:ring-offset-0 focus:ring-redish-400/60 dark:focus:ring-redish-500/50"
                />
                <button
                  type="button"
                  @click="applyActionFilter"
                  class="absolute right-2 top-1/2 -translate-y-1/2 text-brownish-400 dark:text-brownish-50 hover:text-brownish-900 dark:hover:text-white transition-colors"
                  :aria-label="$t('common.search')"
                >
                  <BaseIcon :path="mdiSearchWeb" :size="15" />
                </button>
              </div>
            </div>
          </CardBoxComponentHeader>

          <div class="overflow-x-auto -mx-4 -mb-4" v-if="paginated">
            <table class="w-full">
              <thead>
                <tr>
                  <th>{{ $t('admin.audit.time') }}</th>
                  <th>{{ $t('admin.audit.user') }}</th>
                  <th>{{ $t('admin.audit.action') }}</th>
                  <th>{{ $t('admin.audit.target') }}</th>
                  <th>{{ $t('admin.sessions.ipAddress') }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="row in rows" :key="row.id">
                  <td class="px-4 py-3 text-sm whitespace-nowrap">
                    {{ formatPrettyDate(row.created_at) }}
                  </td>
                  <td class="px-4 py-3 text-sm break-all">
                    <span v-if="row.user_id" :title="row.user_id">
                      {{ row.user_id.substring(0, 8) }}
                    </span>
                    <span v-else class="text-brownish-400 dark:text-brownish-50">—</span>
                  </td>
                  <td class="px-4 py-3 text-sm font-mono">{{ row.action }}</td>
                  <td class="px-4 py-3 text-sm break-all">
                    {{ row.target || '—' }}
                  </td>
                  <td class="px-4 py-3 text-sm">{{ row.ip || '—' }}</td>
                </tr>
                <tr v-if="rows.length === 0">
                  <td colspan="5" class="px-4 py-6 text-center text-sm text-brownish-400 dark:text-brownish-50">
                    {{ $t('admin.audit.empty') }}
                  </td>
                </tr>
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
      </SectionMain>
    </Suspense>
  </LayoutAdminWithLoader>
</template>
