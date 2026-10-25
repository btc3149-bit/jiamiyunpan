import Api from '../api'
import * as cryptfns from '../cryptfns'
import { emitFileTreeChange } from './events'

import type {
  AppFile,
  KeyPair,
  RestoreTrashPayload,
  RestoreTrashResult,
  TrashItem,
  TrashPage,
  TrashParent
} from 'types'

/**
 * Trash (soft delete) wire layer.
 *
 * Every endpoint is owner-only server-side; names arrive encrypted and are
 * decrypted here with the caller's private key, exactly like the file
 * browser does. The one piece of logic that cannot live server-side is the
 * collision rename: the server can detect a name clash (via the keyed
 * `name_hash`) but cannot compute the " (restored)" suffix over ciphertext,
 * so a 409 is answered by re-encrypting the suffixed name and retrying.
 */

export async function list(limit = 50, offset = 0): Promise<TrashPage> {
  const response = await Api.get<TrashPage>(`/api/storage/trash`, { limit, offset })

  return response?.body || { items: [], total: 0, limit, offset }
}

export async function restore(
  fileId: string,
  payload: RestoreTrashPayload = {}
): Promise<RestoreTrashResult> {
  const response = await Api.post<RestoreTrashPayload, RestoreTrashResult>(
    `/api/storage/trash/${fileId}/restore`,
    undefined,
    payload
  )

  if (!response?.body?.restored) {
    throw new Error('Failed to restore file')
  }

  // The file is back in the live tree — nudge the sidebar and any open
  // browser so the row shows up without a manual refresh.
  emitFileTreeChange({ type: 'created' })

  return response.body
}

export async function purge(fileId: string): Promise<void> {
  await Api.delete(`/api/storage/trash/${fileId}`)
  emitFileTreeChange({ type: 'deleted' })
}

export async function empty(): Promise<void> {
  await Api.post<undefined, undefined>(`/api/storage/trash/empty`)
  emitFileTreeChange({ type: 'deleted' })
}

/**
 * Decrypt the caller-visible parts of a trash entry: the file's own name
 * and the plaintext path built from its ancestors' names.
 *
 * A single entry whose key wrap fails to unwrap still renders — with its
 * id as a placeholder name and no path — instead of killing the page.
 */
export async function decryptItem(
  item: TrashItem,
  privateKey: string
): Promise<{ file: AppFile; path: string }> {
  const file: AppFile = { ...item.file, name: item.file.id }

  try {
    const unencrypted = await import('./meta')
    const parts = await unencrypted.decrypt(
      {
        cipher: item.file.cipher,
        encrypted_key: item.file.encrypted_key,
        encrypted_name: item.file.encrypted_name
      },
      privateKey
    )
    file.name = parts.name
  } catch {
    // Unreadable name — the row stays listed and purgeable.
  }

  const segments: string[] = []
  for (const parent of item.parents) {
    try {
      const unencrypted = await import('./meta')
      const parts = await unencrypted.decrypt(
        {
          cipher: parent.cipher,
          encrypted_key: parent.encrypted_key,
          encrypted_name: parent.encrypted_name
        },
        privateKey
      )
      segments.push(parts.name)
    } catch {
      segments.push(parent.id)
    }
  }

  return { file, path: segments.join(' / ') }
}

/**
 * Pure tag math for a collision retry: the keyed name hash and the
 * name-source search tags for the suffixed name, derived exactly the way
 * `rename` derives them for live files. Unit-testable without crypto I/O —
 * `searchTag`/`searchTags` are HMAC-style keyed derivations.
 */
export function buildRestoreRenameTags(
  name: string,
  key: Uint8Array,
  keypair: KeyPair
): { name_hash: string; search_tokens_root: string[]; search_tokens_file: string[] } {
  const rootKey = cryptfns.searchRootKey(keypair)
  const fileKey = cryptfns.searchFileKey(key)
  const indexed = name.toLowerCase()

  return {
    name_hash: cryptfns.searchTag(rootKey, name),
    search_tokens_root: cryptfns.searchTags(rootKey, indexed),
    search_tokens_file: cryptfns.searchTags(fileKey, indexed)
  }
}

/**
 * Full rename payload for a collision retry: the tags above plus the
 * re-encrypted name, wrapped under the same public key the file's key was
 * originally wrapped with.
 */
export async function buildRestoreRenamePayload(
  name: string,
  key: Uint8Array,
  keypair: KeyPair
): Promise<RestoreTrashPayload> {
  const wrapPub = keypair.wrappingPublic || keypair.publicKey
  const { encrypt } = await import('./meta')
  const parts = await encrypt({ key, name }, wrapPub as string)

  return {
    encrypted_name: parts.encrypted_name,
    ...buildRestoreRenameTags(name, key, keypair)
  }
}

export type { TrashParent }
