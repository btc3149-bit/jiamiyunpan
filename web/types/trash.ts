import type { AppFile } from './file'

/**
 * Soft-delete marker surfaced by `GET /api/storage/trash`. The server
 * never sees plaintext names — every trash entry carries the same
 * encrypted metadata the file browser gets, plus the deletion timestamp
 * and the encrypted ancestor chain for rendering the original path.
 */
export interface TrashParent {
  id: string
  encrypted_name: string
  cipher: string
  encrypted_key: string
}

export interface TrashItem {
  file: AppFile
  /** Unix seconds at which the file (or its trash root) was deleted. */
  deleted_at: number
  /** Ancestor folders, root-first, excluding the file itself. */
  parents: TrashParent[]
}

export interface TrashPage {
  items: TrashItem[]
  total: number
  limit: number
  offset: number
}

/**
 * Optional rename payload for a restore. Only needed when the server
 * answers `409 trash_restore_name_conflict`: names are encrypted, so the
 * client computes the " (restored)" suffix and re-encrypts it.
 */
export interface RestoreTrashPayload {
  encrypted_name?: string
  name_hash?: string
  search_tokens_root?: string[]
  search_tokens_file?: string[]
}

export interface RestoreTrashResult {
  restored: boolean
  moved_to_root: boolean
  renamed: boolean
}
