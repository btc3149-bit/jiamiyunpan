import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { mount } from '@vue/test-utils'

import TableFileRow from '../../src/views/files/index-view/TableFileRow.vue'
import { SHARED_WITH_ME_DIR_ID } from '../../services/storage'
import type { AppFile } from '../../types'

const OWNER_ID = 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa'

const SIZES = {
  checkbox: 'pl-2 pt-3 w-10 shrink-0',
  name: 'flex-1 p-2 pt-3 min-w-0 flex',
  size: 'hidden p-2 pt-3 md:block w-24 shrink-0',
  type: 'hidden p-2 pt-3 xl:block w-24 shrink-0',
  modifiedAt: 'hidden p-2 pt-3 sm:block w-44 shrink-0',
  buttons: 'w-10 p-2 shrink-0'
}

function baseFile(overrides: Partial<AppFile> = {}): AppFile {
  return {
    id: '11111111-1111-1111-1111-111111111111',
    user_id: OWNER_ID,
    is_owner: true,
    name: 'doc.pdf',
    name_hash: '',
    mime: 'application/pdf',
    chunks: 0,
    file_id: null,
    file_modified_at: 0,
    created_at: 0,
    finished_upload_at: 1_700_000_000,
    is_new: false,
    editable: false,
    active_version: 1,
    encrypted_key: '',
    encrypted_name: '',
    cipher: 'aegis128l',
    ...overrides
  } as unknown as AppFile
}

function mountRow(file: AppFile): ReturnType<typeof mount> {
  return mount(TableFileRow, {
    props: { file, checkedIds: new Set<string>(), sizes: SIZES }
  })
}

beforeEach(() => {
  setActivePinia(createPinia())
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('TableFileRow: right-click context menu', () => {
  it('emits context-menu with the file and the pointer event', async () => {
    const wrapper = mountRow(baseFile())
    await wrapper.find('[data-testid="file-row-doc.pdf"]').trigger('contextmenu')

    const payload = wrapper.emitted('context-menu')![0]
    expect((payload[0] as AppFile).id).toBe('11111111-1111-1111-1111-111111111111')
    expect(payload[1]).toBeInstanceOf(MouseEvent)
  })

  it('does not open on the synthetic shared-with-me root', async () => {
    const wrapper = mountRow(
      baseFile({ id: SHARED_WITH_ME_DIR_ID, mime: 'dir', name: 'Shared with me' })
    )
    await wrapper.find('div').trigger('contextmenu')
    expect(wrapper.emitted('context-menu')).toBeFalsy()
  })

  it('does not change the selection when right-clicking', async () => {
    const wrapper = mountRow(baseFile())
    await wrapper.find('[data-testid="file-row-doc.pdf"]').trigger('contextmenu')

    expect(wrapper.emitted('select-one')).toBeFalsy()
    expect(wrapper.emitted('deselect-all')).toBeFalsy()
  })
})
