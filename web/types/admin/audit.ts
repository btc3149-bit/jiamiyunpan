import type { Query } from '!/api'

export interface AuditLog {
  id: number
  user_id: string | null
  action: string
  target: string | null
  ip: string | null
  user_agent: string | null
  metadata: string | null
  created_at: number
}

export interface Search extends Query {
  user_id?: string
  /** Action prefix filter, e.g. `login` or `admin.user` */
  action?: string
  /** Inclusive lower bound on created_at (unix seconds) */
  created_from?: number
  /** Inclusive upper bound on created_at (unix seconds) */
  created_to?: number
  limit?: number
  offset?: number
}
