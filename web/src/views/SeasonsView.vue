<script setup lang="ts">
import { adminApi } from '@/lib/api'
import ImageListView from '@/components/ImageListView.vue'
</script>

<template>
  <ImageListView
    kind="season"
    :list-fn="adminApi.getSeasons"
    :image-fn="adminApi.getSeasonImage"
    :fetch-fn="adminApi.fetchSeason"
  />
</template>
