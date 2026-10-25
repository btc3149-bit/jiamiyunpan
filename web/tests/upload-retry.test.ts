import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { mount } from '@vue/test-utils'

// The dispatch never settles — it stands in for the transfer-token round
// trip the worker makes before it acknowledges the file it was handed.
vi.mock('../services/storage/workers', () => ({
  pushUploadToWorker: () => new Promise<void>(() => undefined),
  startFileDownload: () => new Promise<void>(() => undefined)
}))

// `retry` resolves the keypair lazily for cancelled transfers; a static
// object keeps the test off the real crypto module.
vi.mock('../services/crypto', () => ({
  store: () => ({
    keypair: { input: 'private-key', publicKey: 'public-key', keySize: 2048 }
  })
}))

// `cancel` deletes the server row and the re-create path looks the surviving
// row up by name — both are stubbed so no request leaves the test.
vi.mock('../services/storage/meta', () => ({
  getByName: vi.fn(async () => ({
    id: 'srv-1',
    name: 'a.bin',
    chunks: 3,
    chunks_stored: 1
  })),
  remove: vi.fn(async () => undefined),
  requestTransferToken: vi.fn(async () => ({ token: 't' }))
}))

import StatusBar from '../src/components/files/io/StatusBar.vue'
import { store as uploadStore } from '../services/storage/upload'
import * as meta from '../services/storage/meta'
import type { FilesStore, UploadAppFile } from '../types'

const storage = {
  dir: undefined,
  getItem: () => undefined,
  updateItem: () => undefined,
  upsertItem: () => undefined,
  removeItem: () => undefined
} as unknown as FilesStore

function makeFile(id: string, overrides: Partial<UploadAppFile> = {}): UploadAppFile {
  return {
    id,
    temporaryId: id,
    name: `${id}.bin`,
    mime: 'application/octet-stream',
    size: 1,
    chunks: 3,
    encrypted_key: 'wrapped-key',
    file: new File(['x'], `${id}.bin`),
    file_id: null,
    ...overrides
  } as unknown as UploadAppFile
}

describe('upload retry', () => {
  let interval: ReturnType<typeof setInterval>

  beforeEach(() => {
    setActivePinia(createPinia())
    vi.useFakeTimers()
  })

  afterEach(() => {
    clearInterval(interval)
    vi.useRealTimers()
    vi.clearAllMocks()
  })

  it('UNIT: a failed upload returns to waiting with its error state reset', async () => {
    const upload = uploadStore()
    const file = makeFile('a', {
      error: { context: 'boom' } as any,
      cancel: true,
      started_upload_at: '2026-01-01T00:00:00Z',
      finished_upload_at: 1
    })
    upload.failed.push(file)

    await upload.retry(file)

    expect(upload.failed).toHaveLength(0)
    expect(upload.waiting).toHaveLength(1)

    const entry = upload.waiting[0] as UploadAppFile
    expect(entry.id).toBe('a')
    expect(entry.error).toBeUndefined()
    expect(entry.cancel).toBe(false)
    expect(entry.started_upload_at).toBeUndefined()
    expect(entry.finished_upload_at).toBeUndefined()
    // A fresh temporaryId keeps the retried transfer's reports from being
    // deduplicated against the stale failed row.
    expect(entry.temporaryId).not.toBe('a')
  })

  it('UNIT: a cancelled upload is rebuilt from the retained File via push', async () => {
    const upload = uploadStore()
    const file = makeFile('dead-row')
    upload.running.push(file)

    // `cancel` deletes the server row and lists the entry as failed, exactly
    // like a user-initiated stop does in the UI.
    await upload.cancel(storage, file)
    expect(upload.failed).toHaveLength(1)
    expect(meta.remove).toHaveBeenCalledWith('dead-row')

    await upload.retry(file)

    expect(upload.failed).toHaveLength(0)
    expect(upload.waiting).toHaveLength(1)

    const entry = upload.waiting[0] as UploadAppFile
    // The rebuilt transfer continues from the surviving server row rather
    // than the deleted one.
    expect(entry.id).toBe('srv-1')
    expect(entry.file).toBeInstanceOf(File)
  })

  it('UNIT: a cancelled shared-folder upload falls back to re-queueing as-is', async () => {
    const upload = uploadStore()
    // Shared-folder entries are placeholders with an empty encrypted_key:
    // a plain `push` would bypass the multi-key fan-out, so retry must not
    // rebuild them.
    const file = makeFile('shared-1', { encrypted_key: '' })
    upload.running.push(file)

    await upload.cancel(storage, file)
    await upload.retry(file)

    expect(meta.getByName).not.toHaveBeenCalled()
    expect(upload.failed).toHaveLength(0)
    expect(upload.waiting).toHaveLength(1)
    expect((upload.waiting[0] as UploadAppFile).id).toBe('shared-1')
  })
})

describe('StatusBar retry button', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('UNIT: failed rows offer retry and it moves the entry back into the queue', async () => {
    const upload = uploadStore()
    const file = makeFile('a', { error: { context: 'boom' } as any })
    upload.failed.push(file)

    const wrapper = mount(StatusBar)
    const button = wrapper.find('button[aria-label="Retry"]')
    expect(button.exists()).toBe(true)

    await button.trigger('click')

    expect(upload.failed).toHaveLength(0)
    expect(upload.waiting).toHaveLength(1)
  })

  it('UNIT: running rows do not offer retry', () => {
    const upload = uploadStore()
    upload.running.push(makeFile('a'))

    const wrapper = mount(StatusBar)

    expect(wrapper.find('button[aria-label="Retry"]').exists()).toBe(false)
  })
})
