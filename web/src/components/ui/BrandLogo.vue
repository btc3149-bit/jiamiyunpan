<script setup lang="ts">
withDefaults(defineProps<{ size?: number }>(), { size: 48 })

// Unique per-instance gradient id: the login card and the sidebar rail can
// both render this logo in the same document, and duplicate SVG ids would
// make the second instance resolve its gradient against the first.
const gid = `brand-logo-grad-${Math.random().toString(36).slice(2, 9)}`
</script>

<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 48 48"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    role="img"
    aria-hidden="true"
  >
    <defs>
      <linearGradient :id="gid" x1="14" y1="10" x2="34" y2="38" gradientUnits="userSpaceOnUse">
        <stop stop-color="#A63446" />
        <stop offset="1" stop-color="#EE8434" />
      </linearGradient>
    </defs>
    <!-- Shield follows the surrounding text color: white on the charcoal
         rail and dark on the light login card, so only the 甲 mark carries
         the brand gradient. -->
    <path
      d="M24 4 L40 10 V23 C40 33.5 33.2 40.6 24 44 C14.8 40.6 8 33.5 8 23 V10 Z"
      stroke="currentColor"
      stroke-width="3"
      stroke-linejoin="round"
    />
    <!-- 甲: box pierced by a vertical stroke that pokes out top and bottom. -->
    <rect
      x="17"
      y="19"
      width="14"
      height="12"
      rx="1.5"
      :stroke="`url(#${gid})`"
      stroke-width="3"
    />
    <path
      d="M24 13 V37"
      :stroke="`url(#${gid})`"
      stroke-width="3"
      stroke-linecap="round"
    />
  </svg>
</template>
