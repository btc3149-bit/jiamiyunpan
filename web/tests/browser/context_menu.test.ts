import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'

import FileContextMenu from '../../src/components/files/browser/FileContextMenu.vue'
import { installCapabilities } from '../helpers/capabilities'
import type { AppFile } from '../../types'

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

function mountMenu() {
  return mount(FileContextMenu, {
    props: { hideDelete: false, share: true },
    global: { stubs: { teleport: true } }
  })
}

const openAt = async (
  wrapper: ReturnType<typeof mountMenu>,
  file: AppFile,
  x = 40,
  y = 60
) => {
  const event = new MouseEvent('contextmenu', { clientX: x, clientY: y, bubbles: true })
  await (wrapper.vm as unknown as { open: (e: MouseEvent, f: AppFile) => Promise<void> }).open(
    event,
    file
  )
  await nextTick()
}

beforeEach(() => {
  document.body.innerHTML = ''
  installCapabilities({
    sharing: { enabled: true, roles: [] },
    fork: true,
    editable_folders: true,
    share_groups: true
  })
})

afterEach(() => {
  document.body.innerHTML = ''
})

describe('FileContextMenu: open and close', () => {
  it('renders nothing until opened', () => {
    const wrapper = mountMenu()
    expect(wrapper.find('[data-testid="file-context-menu"]').exists()).toBe(false)
  })

  it('opens at the cursor and exposes the same action set as the row dropdown', async () => {
    const wrapper = mountMenu()
    await openAt(wrapper, baseFile())

    const menu = wrapper.find('[data-testid="file-context-menu"]')
    expect(menu.exists()).toBe(true)
    expect(menu.attributes('role')).toBe('menu')

    // Same buttons ActionsButtons renders in the row dropdown for an
    // owned, finished upload: details, rename, download, sharing, delete.
    for (const name of ['details', 'rename', 'download', 'sharing', 'delete']) {
      expect(wrapper.find(`button[name="${name}"]`).exists()).toBe(true)
    }
  })

  it('positions the panel at the pointer coordinates', async () => {
    const wrapper = mountMenu()
    await openAt(wrapper, baseFile(), 120, 80)

    const menu = wrapper.find('[data-testid="file-context-menu"]').element as HTMLElement
    expect(menu.style.left).toBe('120px')
    expect(menu.style.top).toBe('80px')
  })

  it('closes on Escape', async () => {
    const wrapper = mountMenu()
    await openAt(wrapper, baseFile())
    expect(wrapper.find('[data-testid="file-context-menu"]').exists()).toBe(true)

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await nextTick()
    expect(wrapper.find('[data-testid="file-context-menu"]').exists()).toBe(false)
  })

  it('closes on scroll (including inner containers via capture)', async () => {
    const wrapper = mountMenu()
    await openAt(wrapper, baseFile())

    // Capture-phase listener must catch scrolls dispatched on any element,
    // not only the window itself.
    const scroller = document.createElement('div')
    document.body.appendChild(scroller)
    scroller.dispatchEvent(new Event('scroll', { bubbles: false }))
    await nextTick()
    expect(wrapper.find('[data-testid="file-context-menu"]').exists()).toBe(false)
  })

  it('closes on outside click', async () => {
    const wrapper = mountMenu()
    await openAt(wrapper, baseFile())

    document.body.click()
    await nextTick()
    expect(wrapper.find('[data-testid="file-context-menu"]').exists()).toBe(false)
  })
})

describe('FileContextMenu: actions', () => {
  it('emits the action for the file and closes the menu', async () => {
    const wrapper = mountMenu()
    const file = baseFile()
    await openAt(wrapper, file)

    await wrapper.find('button[name="rename"]').trigger('click')
    // The child receives the prop through Vue's reactive props proxy, so
    // compare structurally rather than by identity.
    expect(wrapper.emitted('rename')).toBeTruthy()
    expect(wrapper.emitted('rename')![0][0]).toMatchObject({ id: file.id })
    expect(wrapper.find('[data-testid="file-context-menu"]').exists()).toBe(false)
  })

  it('emits download and remove with the file', async () => {
    const wrapper = mountMenu()
    const file = baseFile()
    await openAt(wrapper, file)

    await wrapper.find('button[name="download"]').trigger('click')
    expect(wrapper.emitted('download')![0][0]).toMatchObject({ id: file.id })

    await openAt(wrapper, file)
    await wrapper.find('button[name="delete"]').trigger('click')
    expect(wrapper.emitted('remove')![0][0]).toMatchObject({ id: file.id })
  })

  it('honours hideDelete like the row dropdown does', async () => {
    const wrapper = mount(FileContextMenu, {
      props: { hideDelete: true, share: true },
      global: { stubs: { teleport: true } }
    })
    await openAt(wrapper, baseFile())
    expect(wrapper.find('button[name="delete"]').exists()).toBe(false)
  })
})
