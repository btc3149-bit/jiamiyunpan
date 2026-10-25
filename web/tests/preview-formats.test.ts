import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import {
  isPreviewable,
  Preview,
  decodeTextPreview,
  TEXT_PREVIEW_MAX_BYTES,
  isTextFile,
  isTextMime,
  getTextExtension,
  type ConstructPreview
} from '../services/preview'
import type { AppFile, AppLink } from 'types'
import PreviewText from '../src/components/preview/PreviewText.vue'
import PreviewAudio from '../src/components/preview/PreviewAudio.vue'
import PreviewDocx from '../src/components/preview/PreviewDocx.vue'

/**
 * The docx pipeline (dynamic mammoth import + conversion) crosses macrotask
 * boundaries, so microtask flushing alone never settles it. Poll with real
 * timers until the predicate holds or the budget runs out.
 */
async function until(predicate: () => boolean, budgetMs = 5000): Promise<void> {
  const deadline = Date.now() + budgetMs
  while (!predicate()) {
    if (Date.now() > deadline) throw new Error('until(): predicate never became true')
    await new Promise((resolve) => setTimeout(resolve, 10))
  }
}

/**
 * Preview is abstract — the feature coverage lives in its concrete methods.
 * Subclass with pluggable content so component tests can drive load()
 * without FilePreview's Pinia store dependency.
 */
class TestPreview extends Preview {
  public bytes: Uint8Array

  constructor(data: Omit<ConstructPreview<Preview>, 'chunks'>, bytes = new Uint8Array()) {
    super(data)
    this.bytes = bytes
  }

  public override async load(): Promise<Uint8Array> {
    return this.bytes
  }
}

/**
 * Build an AppFile with just the fields the preview gate looks at.
 */
function makeFile(partial: Partial<AppFile>): AppFile {
  return {
    id: 'f1',
    user_id: 'u1',
    is_owner: true,
    name_hash: 'hash',
    name: 'file.bin',
    mime: 'application/octet-stream',
    size: 100,
    chunks: 1,
    encrypted_key: '',
    encrypted_name: '',
    cipher: 'aegis-128l',
    editable: false,
    active_version: 1,
    file_modified_at: 0,
    created_at: 0,
    finished_upload_at: 1_700_000_000,
    is_new: false,
    ...partial
  } as AppFile
}

describe('text format helpers', () => {
  it('UNIT: text/* mimes are text', () => {
    expect(isTextMime('text/plain')).toBe(true)
    expect(isTextMime('text/csv')).toBe(true)
    expect(isTextMime('text/html')).toBe(true)
  })

  it('UNIT: known application/* text payloads are text', () => {
    expect(isTextMime('application/json')).toBe(true)
    expect(isTextMime('application/xml')).toBe(true)
    expect(isTextMime('application/javascript')).toBe(true)
    expect(isTextMime('application/x-yaml')).toBe(true)
  })

  it('UNIT: binary mimes are not text', () => {
    expect(isTextMime('application/octet-stream')).toBe(false)
    expect(isTextMime('application/pdf')).toBe(false)
    expect(isTextMime('image/png')).toBe(false)
  })

  it('UNIT: extension extraction handles dotfiles and plain names', () => {
    expect(getTextExtension('script.ts')).toBe('ts')
    expect(getTextExtension('a/b/c/.env')).toBe('env')
    expect(getTextExtension('Makefile')).toBe('makefile')
    expect(getTextExtension('Makefile'.toUpperCase())).toBe('makefile')
    expect(getTextExtension(undefined)).toBe('')
  })

  it('UNIT: octet-stream falls back to the extension allowlist', () => {
    for (const name of ['a.py', 'x.rs', 'server.log', 'config.yml', 'data.json', '.gitignore', 'main.go']) {
      expect(isTextFile({ mime: 'application/octet-stream', name }), name).toBe(true)
    }
    expect(isTextFile({ mime: 'application/octet-stream', name: 'a.bin' })).toBe(false)
    expect(isTextFile({ mime: 'application/octet-stream', name: 'archive.tar.gz' })).toBe(false)
  })

  it('UNIT: .md routes to markdown, not the plain-text viewer', () => {
    expect(isTextFile({ mime: 'application/octet-stream', name: 'README.md' })).toBe(false)
    // text/markdown still satisfies the mime branch, but previewType()
    // checks markdown first — covered below.
  })
})

describe('isPreviewable — new formats', () => {
  it('UNIT: text/* files are previewable', () => {
    expect(isPreviewable(makeFile({ name: 'notes.txt', mime: 'text/plain' }))).toBe(true)
  })

  it('UNIT: JSON/YAML by application mime are previewable', () => {
    expect(isPreviewable(makeFile({ name: 'data.json', mime: 'application/json' }))).toBe(true)
    expect(isPreviewable(makeFile({ name: 'ci.yml', mime: 'application/x-yaml' }))).toBe(true)
  })

  it('UNIT: code files stored as octet-stream are previewable via extension', () => {
    expect(isPreviewable(makeFile({ name: 'main.py', mime: 'application/octet-stream' }))).toBe(true)
    expect(isPreviewable(makeFile({ name: 'app.log', mime: 'application/octet-stream' }))).toBe(true)
  })

  it('UNIT: octet-stream with unknown extension is still rejected', () => {
    expect(isPreviewable(makeFile({ name: 'blob.bin', mime: 'application/octet-stream' }))).toBe(false)
  })

  it('UNIT: audio files are previewable', () => {
    expect(isPreviewable(makeFile({ name: 'song.mp3', mime: 'audio/mpeg' }))).toBe(true)
    expect(isPreviewable(makeFile({ name: 'voice.ogg', mime: 'audio/ogg' }))).toBe(true)
  })

  it('UNIT: docx is previewable', () => {
    expect(
      isPreviewable(
        makeFile({ mime: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' })
      )
    ).toBe(true)
  })

  it('UNIT: zero-size and unfinished uploads stay gated for new types', () => {
    expect(isPreviewable(makeFile({ name: 'a.txt', mime: 'text/plain', size: 0 }))).toBe(false)
    expect(
      isPreviewable(makeFile({ name: 'a.mp3', mime: 'audio/mpeg', finished_upload_at: undefined }))
    ).toBe(false)
  })

  it('UNIT: text and audio links are previewable', () => {
    function makeLink(partial: Partial<AppLink>): AppLink {
      return {
        id: 'l1',
        link_key_hashed: '',
        signature: '',
        file_id: 'f1',
        file_mime: 'application/pdf',
        file_size: 100,
        name: 'file.bin',
        ...partial
      } as AppLink
    }

    expect(isPreviewable(makeLink({ file_mime: 'text/plain', name: 'a.txt' }))).toBe(true)
    expect(isPreviewable(makeLink({ file_mime: 'audio/mpeg', name: 'a.mp3' }))).toBe(true)
    expect(
      isPreviewable(
        makeLink({
          file_mime: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
          name: 'a.docx'
        })
      )
    ).toBe(true)
    expect(isPreviewable(makeLink({ file_mime: 'application/octet-stream', name: 'a.bin' }))).toBe(false)
  })
})

describe('Preview.previewType — new formats', () => {
  it('UNIT: returns "text" for text/plain', () => {
    const p = new TestPreview({ id: '1', name: 'a.txt', mime: 'text/plain', size: 10, editable: false })
    expect(p.previewType()).toBe('text')
  })

  it('UNIT: returns "text" for octet-stream with a code extension', () => {
    const p = new TestPreview({ id: '1', name: 'main.rs', mime: 'application/octet-stream', size: 10, editable: false })
    expect(p.previewType()).toBe('text')
  })

  it('UNIT: markdown still wins over the text branch', () => {
    const p = new TestPreview({ id: '1', name: 'doc.md', mime: 'text/markdown', size: 10, editable: false })
    expect(p.previewType()).toBe('markdown')
  })

  it('UNIT: returns "audio" for audio/*', () => {
    const p = new TestPreview({ id: '1', name: 's.mp3', mime: 'audio/mpeg', size: 10, editable: false })
    expect(p.previewType()).toBe('audio')
    expect(p.isAudio()).toBe(true)
    expect(p.isText()).toBe(false)
  })

  it('UNIT: returns "docx" for Word documents', () => {
    const p = new TestPreview({
      id: '1',
      name: 'a.docx',
      mime: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
      size: 10,
      editable: false
    })
    expect(p.previewType()).toBe('docx')
  })

  it('UNIT: returns null for unrelated binaries', () => {
    const p = new TestPreview({ id: '1', name: 'a.bin', mime: 'application/octet-stream', size: 10, editable: false })
    expect(p.previewType()).toBeNull()
  })
})

describe('decodeTextPreview', () => {
  it('UNIT: small content decodes whole, no truncation', () => {
    const data = new TextEncoder().encode('hello world')
    const result = decodeTextPreview(data)
    expect(result.text).toBe('hello world')
    expect(result.truncated).toBe(false)
  })

  it('UNIT: content over the cap is cut and flagged', () => {
    const big = new TextEncoder().encode('a'.repeat(TEXT_PREVIEW_MAX_BYTES + 123))
    const result = decodeTextPreview(big)
    expect(result.truncated).toBe(true)
    expect(result.text.length).toBeLessThanOrEqual(TEXT_PREVIEW_MAX_BYTES)
  })

  it('UNIT: content exactly at the cap is not truncated', () => {
    const exact = new TextEncoder().encode('a'.repeat(TEXT_PREVIEW_MAX_BYTES))
    const result = decodeTextPreview(exact)
    expect(result.truncated).toBe(false)
  })

  it('UNIT: invalid UTF-8 degrades instead of throwing', () => {
    const garbage = new Uint8Array([0x68, 0x69, 0xff, 0xfe, 0x00])
    const result = decodeTextPreview(garbage)
    expect(result.truncated).toBe(false)
    expect(result.text.startsWith('hi')).toBe(true)
  })

  it('UNIT: multibyte runes survive a byte cut without throwing', () => {
    const runes = new TextEncoder().encode('你好'.repeat(10))
    const result = decodeTextPreview(runes, runes.length - 1)
    expect(result.truncated).toBe(true)
    expect(typeof result.text).toBe('string')
  })
})

describe('PreviewText component', () => {
  it('UNIT: renders decrypted text escaped inside <pre>', async () => {
    const preview = new TestPreview(
      { id: '1', name: 'evil.html', mime: 'text/html', size: 100, editable: false },
      new TextEncoder().encode('<img src=x onerror=alert(1)>')
    )
    const wrapper = mount(PreviewText, { props: { modelValue: preview } })
    await flushPromises()
    expect(wrapper.find('[data-testid="text-preview-content"]').exists()).toBe(true)
    const pre = wrapper.find('[data-testid="text-preview-content"]')
    // Content is interpolated, never injected: the markup shows up escaped
    // as text, and no <img> element exists in the DOM.
    expect(pre.text()).toContain('<img src=x onerror=alert(1)>')
    expect(pre.find('img').exists()).toBe(false)
    expect(wrapper.find('img').exists()).toBe(false)
    expect(wrapper.find('[data-testid="text-preview-truncated"]').exists()).toBe(false)
  })

  it('UNIT: shows the truncation notice for oversized content', async () => {
    const big = 'x'.repeat(TEXT_PREVIEW_MAX_BYTES + 50)
    const preview = new TestPreview(
      { id: '1', name: 'big.log', mime: 'text/plain', size: big.length, editable: false },
      new TextEncoder().encode(big)
    )
    const wrapper = mount(PreviewText, { props: { modelValue: preview } })
    await flushPromises()
    expect(wrapper.find('[data-testid="text-preview-content"]').text().length).toBeGreaterThan(0)
    expect(wrapper.find('[data-testid="text-preview-truncated"]').exists()).toBe(true)
  })
})

describe('PreviewAudio component', () => {
  let createObjectURL: ReturnType<typeof vi.fn>
  let revokeObjectURL: ReturnType<typeof vi.fn>

  beforeEach(() => {
    createObjectURL = vi.fn(() => 'blob:test-url')
    revokeObjectURL = vi.fn()
    Object.defineProperty(URL, 'createObjectURL', { value: createObjectURL, configurable: true, writable: true })
    Object.defineProperty(URL, 'revokeObjectURL', { value: revokeObjectURL, configurable: true, writable: true })
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('UNIT: builds a blob URL from decrypted bytes and revokes it on unmount', async () => {
    const bytes = new TextEncoder().encode('fake-mp3-bytes')
    const preview = new TestPreview(
      { id: '1', name: 'song.mp3', mime: 'audio/mpeg', size: bytes.length, editable: false },
      bytes
    )
    const wrapper = mount(PreviewAudio, { props: { modelValue: preview } })

    await flushPromises()
    expect(createObjectURL).toHaveBeenCalled()
    const audio = wrapper.find('[data-testid="audio-preview-player"]')
    expect(audio.exists()).toBe(true)
    expect(audio.attributes('src')).toBe('blob:test-url')

    wrapper.unmount()
    expect(revokeObjectURL).toHaveBeenCalledWith('blob:test-url')
  })

  it('UNIT: a load failure renders the error state instead of a player', async () => {
    const preview = new TestPreview(
      { id: '1', name: 'broken.flac', mime: 'audio/flac', size: 10, editable: false }
    )
    vi.spyOn(preview, 'load').mockRejectedValue(new Error('boom'))

    const wrapper = mount(PreviewAudio, { props: { modelValue: preview } })
    await flushPromises()
    expect(wrapper.find('[data-testid="audio-preview-error"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="audio-preview-player"]').exists()).toBe(false)
  })
})

describe('PreviewDocx component', () => {
  /**
   * Minimal-but-valid docx: mammoth only needs the package relationships
   * and word/document.xml to produce a paragraph.
   */
  async function makeDocx(bodyText: string): Promise<Uint8Array> {
    const { default: JSZip } = await import('jszip')
    const zip = new JSZip()
    zip.file(
      '[Content_Types].xml',
      `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/></Types>`
    )
    zip.file(
      '_rels/.rels',
      `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>`
    )
    zip.file(
      'word/document.xml',
      `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>${bodyText}</w:t></w:r></w:p></w:body></w:document>`
    )
    return zip.generateAsync({ type: 'uint8array' })
  }

  it('UNIT: renders decrypted docx body text through mammoth', async () => {
    const bytes = await makeDocx('Hello from a real document')
    const preview = new TestPreview(
      { id: '1', name: 'doc.docx', mime: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', size: bytes.length, editable: false },
      bytes
    )
    const wrapper = mount(PreviewDocx, { props: { modelValue: preview } })
    await until(() => wrapper.find('.docx-preview-page').exists() || wrapper.find('[data-testid="docx-preview-error"]').exists())
    const page = wrapper.find('.docx-preview-page')
    expect(page.exists()).toBe(true)
    expect(page.text()).toContain('Hello from a real document')
    expect(page.find('script').exists()).toBe(false)
  })

  it('UNIT: a corrupt document lands in the error state, not a half-render', async () => {
    const bytes = new TextEncoder().encode('this is not a zip file')
    const preview = new TestPreview(
      { id: '1', name: 'doc.docx', mime: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', size: bytes.length, editable: false },
      bytes
    )
    const wrapper = mount(PreviewDocx, { props: { modelValue: preview } })
    await until(() => wrapper.find('.docx-preview-page').exists() || wrapper.find('[data-testid="docx-preview-error"]').exists())
    expect(wrapper.find('.docx-preview-page').exists()).toBe(false)
    expect(wrapper.find('[data-testid="docx-preview-error"]').exists()).toBe(true)
  })
})
