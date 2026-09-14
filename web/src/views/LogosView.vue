<script setup lang="ts">
import { adminApi } from '@/lib/api'
import ImageListView from '@/components/ImageListView.vue'
</script>

<template>
  <ImageListView
    kind="logo"
    :list-fn="adminApi.getLogos"
    :image-fn="adminApi.getLogoImage"
    :fetch-fn="adminApi.fetchLogo"
    :delete-fn="adminApi.purgeLogo"
    :clear-all-fn="adminApi.clearLogos"
  />
</template>
