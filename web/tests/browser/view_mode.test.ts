import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'

import ViewModeToggle from '../../src/components/files/browser/ViewModeToggle.vue'

const STORAGE_KEY = 'files.view-mode'

beforeEach(() => {
  localStorage.clear()
  vi.resetModules()
})

afterEach(() => {
  localStorage.clear()
})

const loadUseViewMode = async () =>
  await import('../../src/views/files/index-view/useViewMode')

describe('useViewMode: persistence', () => {
  it('defaults to table when nothing is stored', async () => {
    const { useViewMode } = await loadUseViewMode()
    const { mode } = useViewMode()
    expect(mode.value).toBe('table')
  })

  it('falls back to table for a stored value it does not know', async () => {
    localStorage.setItem(STORAGE_KEY, 'banana')
    const { useViewMode } = await loadUseViewMode()
    const { mode } = useViewMode()
    expect(mode.value).toBe('table')
  })

  it('restores a stored grid choice', async () => {
    localStorage.setItem(STORAGE_KEY, 'grid')
    const { useViewMode } = await loadUseViewMode()
    const { mode } = useViewMode()
    expect(mode.value).toBe('grid')
  })

  it('writes the choice to localStorage under files.view-mode', async () => {
    const { useViewMode } = await loadUseViewMode()
    const { mode } = useViewMode()

    mode.value = 'grid'
    await Promise.resolve()
    expect(localStorage.getItem(STORAGE_KEY)).toBe('grid')

    mode.value = 'table'
    await Promise.resolve()
    expect(localStorage.getItem(STORAGE_KEY)).toBe('table')
  })

  it('shares one singleton ref across consumers', async () => {
    const { useViewMode } = await loadUseViewMode()
    const a = useViewMode()
    const b = useViewMode()

    a.mode.value = 'grid'
    expect(b.mode.value).toBe('grid')
  })
})

describe('ViewModeToggle', () => {
  const mountToggle = (modelValue: 'table' | 'grid') =>
    mount(ViewModeToggle, { props: { modelValue } })

  it('renders both switcher buttons', () => {
    const wrapper = mountToggle('table')
    expect(wrapper.find('[data-testid="view-mode-table"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="view-mode-grid"]').exists()).toBe(true)
  })

  it('emits table when the table button is pressed in grid mode', async () => {
    const wrapper = mountToggle('grid')
    await wrapper.find('[data-testid="view-mode-table"]').trigger('click')
    expect(wrapper.emitted('update:modelValue')![0]).toEqual(['table'])
  })

  it('emits grid when the grid button is pressed in table mode', async () => {
    const wrapper = mountToggle('table')
    await wrapper.find('[data-testid="view-mode-grid"]').trigger('click')
    expect(wrapper.emitted('update:modelValue')![0]).toEqual(['grid'])
  })

  it('does not emit when the current mode is selected again', async () => {
    const wrapper = mountToggle('table')
    await wrapper.find('[data-testid="view-mode-table"]').trigger('click')
    expect(wrapper.emitted('update:modelValue')).toBeFalsy()
  })

  it('marks the active mode with aria-pressed', () => {
    const wrapper = mountToggle('grid')
    expect(wrapper.find('[data-testid="view-mode-grid"]').attributes('aria-pressed')).toBe(
      'true'
    )
    expect(wrapper.find('[data-testid="view-mode-table"]').attributes('aria-pressed')).toBe(
      'false'
    )
  })
})
