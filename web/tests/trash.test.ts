import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'

/**
 * Trash (soft delete) client suite.
 *
 * The wire layer is exercised against a mocked Api to pin the endpoint
 * shapes the server routes define; the rename-tag math runs for real in
 * WASM; DeleteModal is mounted to prove the confirmation gates survived
 * the move-to-trash wording change.
 */

vi.mock('../services/api', () => {
  const get = vi.fn()
  const post = vi.fn()
  const del = vi.fn()
  return {
    default: class {
      constructor() {}
      toJson() {
        return {}
      }
      static get = get
      static post = post
      static delete = del
    }
  }
})

import Api from '../services/api'
import * as trash from '../services/storage/trash'
import * as cryptfns from '../services/cryptfns'
import DeleteModal from '../src/components/files/modals/DeleteModal.vue'

import type { AppFile, KeyPair, TrashItem } from '../types'

const ApiGetMock = (Api as unknown as { get: ReturnType<typeof vi.fn> }).get
const ApiPostMock = (Api as unknown as { post: ReturnType<typeof vi.fn> }).post
const ApiDeleteMock = (Api as unknown as { delete: ReturnType<typeof vi.fn> }).delete

let kp: Awaited<ReturnType<typeof cryptfns.rsa.generateKeyPair>>

beforeAll(async () => {
  kp = await cryptfns.rsa.generateKeyPair()
})

beforeEach(() => {
  ApiGetMock.mockReset()
  ApiPostMock.mockReset()
  ApiDeleteMock.mockReset()
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('trash wire layer', () => {
  it('lists with limit/offset and falls back to an empty page', async () => {
    ApiGetMock.mockResolvedValue({ body: { items: [], total: 3, limit: 50, offset: 0 } })
    const page = await trash.list()
    expect(ApiGetMock).toHaveBeenCalledWith('/api/storage/trash', { limit: 50, offset: 0 })
    expect(page.total).toBe(3)

    ApiGetMock.mockReset()
    ApiGetMock.mockResolvedValue(undefined)
    const fallback = await trash.list(10, 20)
    expect(ApiGetMock).toHaveBeenCalledWith('/api/storage/trash', { limit: 10, offset: 20 })
    expect(fallback).toEqual({ items: [], total: 0, limit: 10, offset: 20 })
  })

  it('restore posts to the per-file endpoint and purge/empty hit theirs', async () => {
    ApiPostMock.mockResolvedValue({ body: { restored: true, moved_to_root: false, renamed: false } })

    await trash.restore('abc-uuid')
    expect(ApiPostMock).toHaveBeenCalledWith(
      '/api/storage/trash/abc-uuid/restore',
      undefined,
      {}
    )

    ApiPostMock.mockResolvedValue({ body: undefined })
    await trash.purge('abc-uuid')
    expect(ApiDeleteMock).toHaveBeenCalledWith('/api/storage/trash/abc-uuid')

    await trash.empty()
    expect(ApiPostMock).toHaveBeenCalledWith('/api/storage/trash/empty')
  })

  it('restore without a restored flag in the response throws', async () => {
    ApiPostMock.mockResolvedValue({ body: undefined })
    await expect(trash.restore('x')).rejects.toThrow('Failed to restore file')
  })

  it('rename tags carry the suffix and differ from the original name', () => {
    const fileKey = new Uint8Array(32).fill(7)

    const original = trash.buildRestoreRenameTags('report.pdf', fileKey, kp!)
    const suffixed = trash.buildRestoreRenameTags('report.pdf (restored)', fileKey, kp!)

    expect(suffixed.name_hash).not.toBe(original.name_hash)
    expect(suffixed.search_tokens_root.length).toBeGreaterThan(0)
    // The suffixed name must tag to exactly what the server-side rename
    // path derives for the same plaintext — determinism is the contract.
    expect(trash.buildRestoreRenameTags('report.pdf (restored)', fileKey, kp!)).toEqual(
      suffixed
    )
  })

  it('rename payload wraps the new name via the file-key encrypt path', async () => {
    const payload = await trash.buildRestoreRenamePayload(
      'report.pdf (restored)',
      new Uint8Array(32).fill(7),
      kp!
    )

    expect(payload.encrypted_name).toBeTruthy()
    expect(payload.name_hash).toBeTruthy()
    expect(payload.search_tokens_file?.length).toBeGreaterThan(0)
  })
})

describe('DeleteModal move-to-trash wording', () => {
  const file = {
    id: 'file-1',
    mime: 'text/plain',
    name: 'notes.txt'
  } as unknown as AppFile

  const folder = {
    id: 'dir-1',
    mime: 'dir',
    name: 'Documents'
  } as unknown as AppFile

  const mountModal = (modelValue: AppFile | undefined, storage: object) =>
    mount(DeleteModal, {
      props: {
        modelValue,
        Storage: storage,
        kp: { input: 'priv', publicKey: 'pub' } as unknown as KeyPair
      }
    })

  it('a file is confirmable in one click and calls remove', async () => {
    const remove = vi.fn().mockResolvedValue(undefined)
    const wrapper = mountModal(file, { remove })

    const button = wrapper.find('button[type="submit"]')
    expect(button.attributes('disabled')).toBeUndefined()

    await button.trigger('click')
    await flushPromises()

    expect(remove).toHaveBeenCalledTimes(1)
  })

  it('a folder holds the confirm shut until its name is typed', async () => {
    const remove = vi.fn().mockResolvedValue(undefined)
    const wrapper = mountModal(folder, { remove })

    const button = wrapper.find('button[type="submit"]')
    await button.trigger('click')
    await flushPromises()
    expect(remove).not.toHaveBeenCalled()

    const input = wrapper.find('[data-testid="delete-confirm-name"]')
    expect(input.exists()).toBe(true)
    await input.setValue('Documents')

    await wrapper.find('button[type="submit"]').trigger('click')
    await flushPromises()
    expect(remove).toHaveBeenCalledTimes(1)
  })
})
