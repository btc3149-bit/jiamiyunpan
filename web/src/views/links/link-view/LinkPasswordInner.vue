<script setup lang="ts">
import LayoutGuest from '@/layouts/LayoutGuest.vue'
import SectionFullScreen from '@/components/ui/SectionFullScreen.vue'
import CardBox from '@/components/ui/CardBox.vue'
import { AppForm, AppField, AppButton } from '@/components/form'
import * as yup from 'yup'
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'

/**
 * Second factor for a password-protected public link. This is NOT the link
 * key: the fragment key decrypts, this password authorises the download.
 * A correct answer exchanges it for a scoped capability cookie via the
 * unlock route, after which the normal download flow works untouched —
 * including the chunk requests built inside the wasm downloader.
 */
defineProps<{
  unlockingError?: string
}>()
const emits = defineEmits<{
  (event: 'unlock', password: string): void
}>()

const { t } = useI18n()

const config = ref({
  initialValues: {
    linkPassword: ''
  },
  validationSchema: yup.object().shape({
    linkPassword: yup.string().required(t('links.protection.passwordRequired'))
  }),
  onSubmit: async (values: { linkPassword: string }) => {
    emits('unlock', values.linkPassword)
  }
})
</script>
<template>
  <LayoutGuest>
    <SectionFullScreen v-slot="{ cardClass }">
      <CardBox :class="cardClass" v-if="config" data-testid="link-password-form">
        <h1 class="text-2xl text-brownish-700 dark:text-white mb-5">{{ $t('links.view.passwordTitle') }}</h1>
        <p>
          {{ $t('links.view.passwordDescription') }}
        </p>

        <AppForm :config="config" class="mt-8 space-y-6" v-slot="{ form }">
          <AppField
            type="password"
            :form="form"
            :label="$t('links.view.passwordLabel')"
            name="linkPassword"
            placeholder="••••••••"
            :autofocus="true"
          />

          <p v-if="unlockingError" class="text-sm text-redish-400 dark:text-redish-100">
            {{ unlockingError }}
          </p>

          <AppButton color="info" :form="form" type="submit">{{ $t('links.view.passwordSubmit') }}</AppButton>
        </AppForm>
      </CardBox>
    </SectionFullScreen>
  </LayoutGuest>
</template>
