import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createRouter, createMemoryHistory } from 'vue-router'

// The link-view chain pulls in services/preview/link which calls the
// Links pinia store at module load. Activate pinia before the dynamic
// import so the side effect lands on a real store.
setActivePinia(createPinia())

import { ErrorResponse } from '../services/api'
import type { AppLink, LinksStore } from '../types'

let LinkViewInner: typeof import('../src/views/links/link-view/LinkViewInner.vue').default
let LinkPasswordInner: typeof import('../src/views/links/link-view/LinkPasswordInner.vue').default

beforeAll(async () => {
  LinkViewInner = (await import('../src/views/links/link-view/LinkViewInner.vue')).default
  LinkPasswordInner = (
    await import('../src/views/links/link-view/LinkPasswordInner.vue')
  ).default
})

function fakeError(status: number, message = 'gone'): ErrorResponse<unknown> {
  return Object.assign(new Error(message), {
    kind: 'ErrorResponse',
    status,
    description: message,
    body: { message },
    request: { method: 'get', url: '/api/links/x/metadata' },
    headers: new Headers(),
    rawBody: undefined,
    validation: null
  }) as unknown as ErrorResponse<unknown>
}

function fakeLink(overrides: Partial<AppLink> = {}): AppLink {
  return {
    id: 'link-id',
    file_id: 'file-id',
    owner_id: 'owner',
    owner_email: 'owner@example.com',
    owner_pubkey: 'pub',
    file_size: 8,
    file_mime: 'text/plain',
    signature: 'sig',
    downloads: 0,
    encrypted_name: 'enc',
    encrypted_link_key: 'enckey',
    link_key_hex: 'abc123',
    created_at: 0,
    file_modified_at: 0,
    file_cipher: 'ascon128a',
    has_password: true,
    ...overrides
  } as AppLink
}

interface StoreSpec {
  link?: AppLink
  getError?: ErrorResponse<unknown>
  unlockError?: ErrorResponse<unknown>
  unlockImpl?: (id: string, password: string) => Promise<void>
}

function fakeLinksStore(spec: StoreSpec): LinksStore {
  return {
    get: vi.fn(async () => {
      if (spec.getError) throw spec.getError
      return spec.link as AppLink
    }),
    unlock: vi.fn(async (id: string, password: string) => {
      if (spec.unlockError) throw spec.unlockError
      if (spec.unlockImpl) return spec.unlockImpl(id, password)
    })
  } as unknown as LinksStore
}

const router = createRouter({
  history: createMemoryHistory(),
  routes: [{ path: '/:catchAll(.*)', component: { template: '<div />' } }]
})

async function mountInner(linkKeyHex: string | undefined, store: LinksStore) {
  // LinkViewInner's `await load()` makes it an async setup component;
  // testing it directly needs a Suspense parent so the resolved tree
  // renders before assertions.
  const Wrapper = {
    components: { LinkViewInner },
    props: ['linkKeyHex'],
    setup(props: { linkKeyHex?: string }) {
      return { props, store }
    },
    template: `
      <Suspense>
        <LinkViewInner :Links="store" id="link-id" :linkKeyHex="props.linkKeyHex" />
      </Suspense>
    `
  }
  const wrapper = mount(Wrapper, {
    global: { plugins: [router] },
    props: { linkKeyHex }
  })
  await flushPromises()
  return wrapper
}

beforeEach(() => {
  setActivePinia(createPinia())
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('LinkViewInner — password-protected links', () => {
  it('shows_the_password_form_when_the_link_has_password', async () => {
    const store = fakeLinksStore({ link: fakeLink() })
    const wrapper = await mountInner('deadbeef', store)

    expect(wrapper.find('[data-testid="link-password-form"]').exists()).toBe(true)
    // The preview must not render before the password is accepted: every
    // gated request behind it would fail anyway.
    expect(wrapper.findComponent({ name: 'PreviewView' }).exists()).toBe(false)
  })

  it('does_not_ask_for_password_on_unprotected_links', async () => {
    const store = fakeLinksStore({ link: fakeLink({ has_password: false }) })
    const wrapper = await mountInner('deadbeef', store)

    expect(wrapper.find('[data-testid="link-password-form"]').exists()).toBe(false)
  })

  it('keeps_the_form_and_shows_an_error_when_the_password_is_wrong', async () => {
    const store = fakeLinksStore({
      link: fakeLink(),
      unlockError: fakeError(401, 'invalid_link_password')
    })
    const wrapper = await mountInner('deadbeef', store)

    const form = wrapper.find('[data-testid="link-password-form"]')
    expect(form.exists()).toBe(true)

    // Wrong password → 401 from the unlock route → error text, form stays.
    store.unlock.mockRejectedValueOnce(fakeError(401, 'invalid_link_password'))
    wrapper.findComponent(LinkPasswordInner).vm.$emit('unlock', 'wrong')
    await flushPromises()

    expect(wrapper.find('[data-testid="link-password-form"]').exists()).toBe(true)
    expect(wrapper.text()).toContain('Wrong password')
  })

  it('proceeds_after_a_successful_unlock', async () => {
    const store = fakeLinksStore({ link: fakeLink() })
    const wrapper = await mountInner('deadbeef', store)

    wrapper.findComponent(LinkPasswordInner).vm.$emit('unlock', 'sesame-42')
    await flushPromises()

    expect(store.unlock).toHaveBeenCalledWith('link-id', 'sesame-42')
    // Vnode-level check: the password form unmounts once unlocked. The
    // DOM-level find is skipped here — the freshly mounted preview inside
    // the Suspense boundary makes test-utils' DOM root walk unreliable.
    expect(wrapper.findComponent(LinkPasswordInner).exists()).toBe(false)
  })

  it('reports_a_spent_download_limit_as_terminal', async () => {
    const store = fakeLinksStore({
      link: fakeLink(),
      unlockError: fakeError(410, 'link_download_limit_reached')
    })
    const wrapper = await mountInner('deadbeef', store)

    wrapper.findComponent(LinkPasswordInner).vm.$emit('unlock', 'sesame-42')
    await flushPromises()

    // 410 is not a "retry" error — the cap is spent for good.
    expect(wrapper.find('[data-testid="link-password-form"]').exists()).toBe(true)
    expect(wrapper.text()).toContain('download limit')
  })
})
