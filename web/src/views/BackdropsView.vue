<script setup lang="ts">
import { adminApi } from '@/lib/api'
import ImageListView from '@/components/ImageListView.vue'
</script>

<template>
  <ImageListView
    kind="backdrop"
    :list-fn="adminApi.getBackdrops"
    :image-fn="adminApi.getBackdropImage"
    :fetch-fn="adminApi.fetchBackdrop"
    :delete-fn="adminApi.purgeBackdrop"
    :clear-all-fn="adminApi.clearBackdrops"
  />
</template>
