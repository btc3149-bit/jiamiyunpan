import Api from '!/api'
import type { BulkAction, BulkRequest, BulkResponse } from 'types/admin/users'

/**
 * Bulk apply an action to many users. The server handles each id
 * independently and never fails the whole batch on one bad id.
 */
export async function usersBulk(action: BulkAction, ids: string[]): Promise<BulkResponse> {
  const body: BulkRequest = { action, ids }
  const response = await Api.post<BulkRequest, BulkResponse>('/api/admin/users/bulk', undefined, body)

  if (!response.body) {
    throw new Error('Failed to run bulk action')
  }

  return response.body
}

/**
 * Bulk delete many files. The server handles each id independently and
 * never fails the whole batch on one bad id.
 */
export async function filesBulk(action: BulkAction, ids: string[]): Promise<BulkResponse> {
  const body: BulkRequest = { action, ids }
  const response = await Api.post<BulkRequest, BulkResponse>('/api/admin/files/bulk', undefined, body)

  if (!response.body) {
    throw new Error('Failed to run bulk action')
  }

  return response.body
}
