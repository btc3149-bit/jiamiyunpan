import { isPreviewable, isMarkdownFile } from '!/preview'
import type { AppFile } from 'types'
import type { Router } from 'vue-router'

/**
 * Navigation rules shared by the table rows and the grid cards so both
 * views open a file exactly the same way. The markdown-editor branch
 * mirrors the gate in `TableFileRow`: recipients may only land in the
 * editor when their share role allows writes.
 */
export const canWriteMarkdown = (file: AppFile): boolean => {
  if (file.is_owner) return true
  const role = file.share_role
  return role === 'editor' || role === 'co-owner'
}

export const openDir = (router: Router, file: AppFile): void => {
  router.push({ name: 'files', params: { file_id: file.id } })
}

/**
 * Finished markdown goes to the editor for users who may write it,
 * previewable files go to the read-only preview, and everything else
 * falls back to the caller's details modal.
 */
export const openFileOrDetails = (
  router: Router,
  file: AppFile,
  onDetails: (file: AppFile) => void
): void => {
  if (file.finished_upload_at && isMarkdownFile(file) && canWriteMarkdown(file)) {
    router.push({ name: 'notes', params: { id: file.id } })
  } else if (isPreviewable(file)) {
    router.push({ name: 'file-preview', params: { id: file.id } })
  } else {
    onDetails(file)
  }
}
