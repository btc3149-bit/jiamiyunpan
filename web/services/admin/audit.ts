import Api from '!/api'
import type { Paginated } from 'types'
import type { AuditLog, Search } from 'types/admin/audit'

/**
 * Get a paginated, newest-first page of the platform audit trail
 * (logins and admin actions) with optional filters.
 */
export async function index(search: Search): Promise<Paginated<AuditLog>> {
  const response = await Api.get<Paginated<AuditLog>>(`/api/admin/audit`, search)

  if (!response.body) {
    throw new Error('Failed to get audit logs')
  }

  return response.body
}
