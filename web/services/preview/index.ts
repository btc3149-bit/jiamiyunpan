import type { AppLink, AppFile } from 'types'
type NonFunctionPropertyNames<T> = { [K in keyof T]: T[K] extends Function ? never : K }[keyof T]
export type ConstructPreview<T> = Pick<T, NonFunctionPropertyNames<T>>

/**
 * Upper bound on how much of a text file the preview renders. Anything
 * beyond this is decoded from the leading slice and announced as truncated,
 * so a stray 200 MB log file can't freeze the tab.
 */
export const TEXT_PREVIEW_MAX_BYTES = 1024 * 1024

/**
 * Extensions we are willing to render as text even when the stored mime is
 * the generic application/octet-stream. `.md` is intentionally absent —
 * markdown files route to the markdown preview, not the plain-text one.
 */
const TEXT_EXTENSIONS = [
  'txt', 'log', 'json', 'xml', 'yml', 'yaml', 'csv', 'tsv', 'ini', 'cfg', 'conf',
  'toml', 'env', 'properties', 'ts', 'tsx', 'js', 'jsx', 'mjs', 'cjs',
  'py', 'rs', 'go', 'java', 'kt', 'swift', 'c', 'h', 'cpp', 'hpp', 'cs', 'rb',
  'php', 'sh', 'bash', 'zsh', 'fish', 'ps1', 'sql', 'html', 'htm', 'css',
  'scss', 'less', 'vue', 'svelte', 'lua', 'pl', 'r', 'dockerfile',
  'gitignore', 'makefile', 'editorconfig'
]

/**
 * application/* mimes that carry plain-text payloads (servers rarely label
 * these text/*, but the content decodes exactly the same way).
 */
const TEXT_APPLICATION_MIMES = [
  'application/json',
  'application/xml',
  'application/xhtml+xml',
  'application/javascript',
  'application/x-javascript',
  'application/x-yaml',
  'application/yaml',
  'application/toml',
  'application/x-sh',
  'application/sql'
]

export function getTextExtension(name: string | undefined): string {
  if (!name) return ''
  const base = name.toLowerCase().split('/').pop() ?? ''
  const dot = base.lastIndexOf('.')
  // Dotfiles like .gitignore/.env carry their "extension" in the basename;
  // extension-less names (e.g. "Makefile") count as their own extension.
  if (dot <= 0) return dot === 0 ? base.slice(1) : base
  return base.slice(dot + 1)
}

/**
 * The text half of the check, split from [[hasPreviewableContent]] so both
 * the raw-mime path and the extension-fallback path stay testable.
 */
export function isTextMime(mime: string): boolean {
  return mime.startsWith('text/') || TEXT_APPLICATION_MIMES.includes(mime)
}

export function isTextFile(item: { mime: string; name?: string }): boolean {
  if (isTextMime(item.mime)) return true
  return item.mime === 'application/octet-stream' && TEXT_EXTENSIONS.includes(getTextExtension(item.name))
}

export function isAudioMime(mime: string): boolean {
  return mime.startsWith('audio/')
}

export function isDocxMime(mime: string): boolean {
  return (
    mime === 'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
  )
}

/**
 * Decode decrypted bytes for the text preview, never reading past
 * [[TEXT_PREVIEW_MAX_BYTES]] so huge files render their head instead of
 * their entirety. Invalid UTF-8 degrades to replacement characters rather
 * than throwing, and the cut point may split a multibyte rune — harmless
 * for a display-only preview.
 */
export function decodeTextPreview(
  data: Uint8Array,
  maxBytes = TEXT_PREVIEW_MAX_BYTES
): { text: string; truncated: boolean } {
  const truncated = data.length > maxBytes
  const slice = truncated ? data.slice(0, maxBytes) : data
  const text = new TextDecoder('utf-8', { fatal: false }).decode(slice)
  return { text, truncated }
}

/**
 * Check if the file or link is previewable
 */
export function isPreviewable(item: AppFile | AppLink): boolean {
  if ((item as AppLink).file_mime) {
    return isLinkPreviewable(item as AppLink)
  }

  return isFilePreviewable(item as AppFile)
}

/**
 * Helper to check if the file is previewable
 */
function isFilePreviewable(item: AppFile): boolean {
  // A file whose chunks never landed has nothing to decrypt — the server
  // answers with an error body and the cipher chokes on it.
  if (!item.finished_upload_at) {
    return false
  }

  return hasPreviewableContent(item)
}

/**
 * The mime/size half of the check, run against [[Preview]] instances too —
 * they carry the same shape but none of the file row's upload state.
 */
function hasPreviewableContent(item: {
  size?: number
  mime: string
  name?: string
  thumbnail?: string
  has_thumbnail?: boolean
}): boolean {
  return (
    !!item.size &&
    item.size > 0 &&
    (item.thumbnail !== undefined ||
      item.has_thumbnail === true ||
      item.mime === 'application/pdf' ||
      item.mime === 'text/markdown' ||
      item.mime === 'text/x-markdown' ||
      isTextFile(item) ||
      isAudioMime(item.mime) ||
      isDocxMime(item.mime) ||
      item.mime.startsWith('image/') ||
      item.mime.startsWith('video/'))
  )
}

/**
 * Check if a file is a markdown file by mime type or extension
 */
export function isMarkdownFile(item: AppFile): boolean {
  return (
    item.mime === 'text/markdown' ||
    item.mime === 'text/x-markdown' ||
    item.name.toLowerCase().endsWith('.md')
  )
}

/**
 * Helper to check if the link is previewable
 */
function isLinkPreviewable(item: AppLink): boolean {
  return (
    !!item.file_size &&
    item.file_size > 0 &&
    (item.thumbnail !== undefined ||
      item.has_thumbnail === true ||
      item.file_mime === 'application/pdf' ||
      isTextFile({ mime: item.file_mime, name: item.name }) ||
      isAudioMime(item.file_mime) ||
      isDocxMime(item.file_mime))
  )
}

/**
 * Base class for previews, contains common properties
 * used for preview of any type of inner file definition (file or link).
 * It also exposes static methods for creating previews from file or link.
 *
 * Each link, or file preview exposes inner functions that provide the actual
 * data for the preview and other actions that can be called on each respectively.
 *
 * @abstract
 */
export abstract class Preview {
  public id: string
  public parentId?: string
  public name: string
  public mime: string
  public size: number | undefined
  public thumbnail?: string
  public editable: boolean

  constructor(data: Omit<ConstructPreview<Preview>, 'chunks'>) {
    this.id = data.id
    this.name = data.name
    this.mime = data.mime
    this.size = data.size
    this.parentId = data.parentId
    this.thumbnail = data.thumbnail
    this.editable = !!data.editable
  }

  /**
   * Get the items that are in the same directory
   * as the main preview file (if applicable)
   */
  public async loadItems(): Promise<void> {
    return
  }

  /**
   * Get total number of items in the same directory
   */
  public getTotal(): number {
    return 0
  }

  /**
   * Get index of the current preview item in the list of other
   * items from the same directory
   */
  public getIndex(): number {
    return -1
  }

  /**
   * Get the id of previous item
   */
  public getPreviousId(): string | undefined {
    return undefined
  }

  /**
   * Get the id of next item
   */
  public getNextId(): string | undefined {
    return undefined
  }

  /**
   * Load the data for the preview.
   *
   * `onBytes` receives the cumulative received byte count as the content
   * streams in, so large files can show movement instead of a bare spinner.
   */
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  public async load(onBytes?: (bytes: number) => void): Promise<Uint8Array> {
    return new Uint8Array()
  }

  /**
   * Thumbnail to hold the frame while the full asset downloads.
   *
   * Subclasses that can source one lazily override this; the base just
   * hands back whatever was passed in at construction.
   */
  public async loadThumbnail(): Promise<string | undefined> {
    return this.thumbnail
  }

  /**
   * Number of encrypted chunks (used for progressive video loading).
   * Overridden by FilePreview; returns undefined for previews without per-chunk access.
   */
  public get chunks(): number | undefined {
    return undefined
  }

  /**
   * Download and decrypt a single chunk by index.
   * Overridden by FilePreview; base implementation returns empty bytes.
   */
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  public async loadChunk(_: number, _signal?: AbortSignal): Promise<Uint8Array> {
    return new Uint8Array()
  }

  /**
   * Easily match the preview type
   */
  public previewType(): 'image' | 'pdf' | 'video' | 'markdown' | 'text' | 'audio' | 'docx' | null {
    if (this.isImage()) return 'image'
    if (this.isPdf()) return 'pdf'
    if (this.isVideo()) return 'video'
    if (this.isMarkdown()) return 'markdown'
    if (this.isText()) return 'text'
    if (this.isAudio()) return 'audio'
    if (this.isDocx()) return 'docx'
    return null
  }

  /**
   * Lets us know if the wrapped file can have a preview at all
   */
  public is(): boolean {
    return hasPreviewableContent(this)
  }

  /**
   * Lets us know if the preview is an image
   */
  public isImage(): boolean {
    return this.is() && this.mime.startsWith('image/')
  }

  /**
   * Lets us know if the preview is a pdf file
   */
  public isPdf(): boolean {
    return this.is() && this.mime === 'application/pdf'
  }

  /**
   * Lets us know if the preview is a video
   */
  public isVideo(): boolean {
    return this.is() && this.mime.startsWith('video/')
  }

  /**
   * Lets us know if the preview is a markdown file
   */
  public isMarkdown(): boolean {
    return (
      this.mime === 'text/markdown' ||
      this.mime === 'text/x-markdown' ||
      this.name.toLowerCase().endsWith('.md')
    )
  }

  /**
   * Lets us know if the preview renders as plain text. Markdown wins over
   * this branch — [[previewType]] checks it first — so .md files keep the
   * rich editor instead of falling into the raw-text viewer.
   */
  public isText(): boolean {
    return this.is() && isTextFile({ mime: this.mime, name: this.name })
  }

  /**
   * Lets us know if the preview is an audio file
   */
  public isAudio(): boolean {
    return this.is() && isAudioMime(this.mime)
  }

  /**
   * Lets us know if the preview is a Word document (docx)
   */
  public isDocx(): boolean {
    return this.is() && isDocxMime(this.mime)
  }
}
