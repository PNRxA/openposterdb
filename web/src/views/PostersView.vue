<script setup lang="ts">
import { adminApi } from '@/lib/api'
import ImageListView from '@/components/ImageListView.vue'
</script>

<template>
  <ImageListView
    kind="poster"
    :list-fn="adminApi.getPosters"
    :image-fn="adminApi.getPosterImage"
    :fetch-fn="adminApi.fetchPoster"
    :delete-fn="adminApi.purgePoster"
    :clear-all-fn="adminApi.clearPosters"
  />
</template>
