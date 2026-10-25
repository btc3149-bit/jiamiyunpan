import { ref, watch } from 'vue'

export type FilesViewMode = 'table' | 'grid'

export const VIEW_MODE_STORAGE_KEY = 'files.view-mode'

const readStored = (): FilesViewMode => {
  try {
    return localStorage.getItem(VIEW_MODE_STORAGE_KEY) === 'grid' ? 'grid' : 'table'
  } catch {
    // Storage can be unavailable (private mode, disabled cookies) — the
    // in-memory default still gives a working session.
    return 'table'
  }
}

/**
 * Module-scope singleton: the files listing and any future surface that
 * wants to honour the same preference read one shared ref, and the watch
 * below is registered exactly once no matter how many components call
 * `useViewMode()`.
 */
const mode = ref<FilesViewMode>(readStored())

watch(mode, (value) => {
  try {
    localStorage.setItem(VIEW_MODE_STORAGE_KEY, value)
  } catch {
    // Same story on write failures — nothing to do, choice stays for the
    // current session only.
  }
})

export const useViewMode = (): { mode: typeof mode } => {
  return { mode }
}
