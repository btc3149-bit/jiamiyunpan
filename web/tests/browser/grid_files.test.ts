import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'

import GridFiles from '../../src/views/files/index-view/GridFiles.vue'
import type { AppFile } from '../../types'

const routerPush = vi.fn()
vi.mock('vue-router', () => ({
  useRouter: () => ({ push: routerPush })
}))

const OWNER_ID = 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa'

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
    file_modified_at: 1_700_000_000,
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

const DIR = baseFile({
  id: '22222222-2222-2222-2222-222222222222',
  name: 'reports',
  mime: 'dir'
})

function mountGrid(
  items: AppFile[] = [DIR, baseFile()],
  checkedIds = new Set<string>(),
  extra: Record<string, unknown> = {}
) {
  return mount(
    GridFiles,
    {
      props: {
        items,
        checkedIds,
        hideDelete: false,
        share: true,
        hideCheckbox: false,
        searchedFileId: undefined
      },
      ...extra
    } as object
  )
}

beforeEach(() => {
  setActivePinia(createPinia())
  routerPush.mockClear()
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('GridFiles: rendering', () => {
  it('renders one card per item', () => {
    const wrapper = mountGrid()
    const cards = wrapper.findAll('[data-testid="file-card"]')
    expect(cards).toHaveLength(2)
    expect(cards[0].attributes('data-file-id')).toBe(DIR.id)
  })

  it('shows the card name with the directory trailing slash', () => {
    const wrapper = mountGrid()
    expect(wrapper.text()).toContain('reports/')
    expect(wrapper.text()).toContain('doc.pdf')
  })

  it('shows size and modified date on file cards', () => {
    const wrapper = mountGrid([baseFile({ size: 2_048 })])
    expect(wrapper.text()).toContain('2.00 KB')
    expect(wrapper.find('[data-testid="file-card"]').text().length).toBeGreaterThan(0)
  })
})

describe('GridFiles: selection', () => {
  it('single click on the card deselects all and toggles the card', async () => {
    const wrapper = mountGrid()
    const card = wrapper.findAll('[data-testid="file-card"]')[1]
    await card.find('button[data-row-nav]').trigger('click')

    expect(wrapper.emitted('deselect-all')).toBeTruthy()
    const payload = wrapper.emitted('select-one')![0]
    expect(payload[0]).toBe(true)
    expect((payload[1] as AppFile).id).toBe('11111111-1111-1111-1111-111111111111')
  })

  it('shift-click emits select-range like the table rows', async () => {
    const wrapper = mountGrid()
    const card = wrapper.findAll('[data-testid="file-card"]')[0]
    await card.find('button[data-row-nav]').trigger('click', { shiftKey: true })

    expect(wrapper.emitted('select-range')![0][0]).toMatchObject({ id: DIR.id })
    expect(wrapper.emitted('select-one')).toBeFalsy()
  })

  it('reflects checked state from checkedIds', async () => {
    const wrapper = mountGrid([baseFile()], new Set(['11111111-1111-1111-1111-111111111111']))
    await nextTick()
    expect(wrapper.find('[data-testid="file-card"]').classes()).toContain('bg-paper-50')
  })
})

describe('GridFiles: open on double click', () => {
  it('navigates into a directory', async () => {
    const wrapper = mountGrid([DIR])
    await wrapper.find('button[data-row-nav]').trigger('dblclick')
    expect(routerPush).toHaveBeenCalledWith({
      name: 'files',
      params: { file_id: DIR.id }
    })
  })

  it('falls back to the details modal for non-previewable files', async () => {
    const wrapper = mountGrid([baseFile({ mime: 'application/octet-stream' })])
    await wrapper.find('button[data-row-nav]').trigger('dblclick')
    expect(routerPush).not.toHaveBeenCalled()
    expect(wrapper.emitted('details')![0][0]).toMatchObject({
      id: '11111111-1111-1111-1111-111111111111'
    })
  })
})

describe('GridFiles: context menu trigger', () => {
  it('right-click emits context-menu with the file and the event', async () => {
    const wrapper = mountGrid()
    const card = wrapper.findAll('[data-testid="file-card"]')[1]
    await card.trigger('contextmenu')

    const payload = wrapper.emitted('context-menu')![0]
    expect((payload[0] as AppFile).id).toBe('11111111-1111-1111-1111-111111111111')
    expect(payload[1]).toBeInstanceOf(MouseEvent)
  })

  it('forwards the per-file dropdown actions from the card', async () => {
    const wrapper = mountGrid([baseFile()])
    // The ActionsDropdown lives inside the sm:block wrapper; its trigger
    // button carries name="actions-dropdown".
    const trigger = wrapper.find('button[name="actions-dropdown"]')
    expect(trigger.exists()).toBe(true)
  })
})

describe('GridFiles: keyboard navigation', () => {
  it('moves focus between cards with the arrow keys', async () => {
    // focus() only works on attached elements, so this suite mounts into
    // the document.
    const wrapper = mountGrid([DIR, baseFile()], new Set(), { attachTo: document.body })
    const buttons = wrapper.findAll('button[data-row-nav]')

    buttons[0].element.focus()
    expect(document.activeElement).toBe(buttons[0].element)

    await buttons[0].trigger('keydown', { key: 'ArrowDown' })
    expect(document.activeElement).toBe(buttons[1].element)

    await buttons[1].trigger('keydown', { key: 'ArrowUp' })
    expect(document.activeElement).toBe(buttons[0].element)

    wrapper.unmount()
    document.body.innerHTML = ''
  })
})
