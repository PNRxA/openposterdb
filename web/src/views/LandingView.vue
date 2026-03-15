<script setup lang="ts">
import { version } from "../../package.json";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Image, KeyRound, Zap, Shield, ExternalLink } from "lucide-vue-next";
import NavButtons from "@/components/NavButtons.vue";
import FreeApiKeyCard from "@/components/FreeApiKeyCard.vue";
import BlurImage from "@/components/BlurImage.vue";

const features = [
  {
    icon: Image,
    title: "Posters & Backdrops",
    desc: "Generate and serve posters, logos, and backdrops for movies, shows, and collections.",
  },
  {
    icon: KeyRound,
    title: "API Key Management",
    desc: "Create and manage API keys with per-key settings for different media servers.",
  },
  {
    icon: Zap,
    title: "Fast & Cached",
    desc: "In-memory caching and on-disk storage for instant poster delivery.",
  },
  {
    icon: Shield,
    title: "RPDB Compatible",
    desc: "Drop-in replacement for RPDB with full API compatibility.",
  },
];

const posters = [
  { src: "/examples/nosferatu.jpg", label: "Nosferatu (1922)" },
  { src: "/examples/metropolis.jpg", label: "Metropolis (1927)" },
  {
    src: "/examples/caligari.jpg",
    alt: "The Cabinet of Dr. Caligari (1920)",
    label: "Dr. Caligari (1920)",
  },
  {
    src: "/examples/phantom.jpg",
    alt: "The Phantom of the Opera (1925)",
    label: "Phantom of the Opera (1925)",
  },
  {
    src: "/examples/trip-to-moon.jpg",
    alt: "A Trip to the Moon (1902)",
    label: "A Trip to the Moon (1902)",
  },
  { src: "/examples/safety-last.jpg", alt: "Safety Last! (1923)", label: "Safety Last! (1923)" },
  { src: "/examples/the-general.jpg", label: "The General (1926)" },
];

const positions = [
  { src: "/examples/pos-tl.jpg", label: "Top left" },
  { src: "/examples/pos-tc.jpg", label: "Top center" },
  { src: "/examples/pos-tr.jpg", label: "Top right" },
  { src: "/examples/pos-r.jpg", label: "Right" },
  { src: "/examples/pos-bl.jpg", label: "Bottom left" },
  { src: "/examples/pos-bc.jpg", label: "Bottom center" },
  { src: "/examples/pos-br.jpg", label: "Bottom right" },
  { src: "/examples/pos-l.jpg", label: "Left" },
];

const styles = [
  { src: "/examples/style-h.jpg", label: "Horizontal" },
  { src: "/examples/style-v.jpg", label: "Vertical" },
];

const labels = [
  { src: "/examples/label-icon.jpg", label: "Icons" },
  { src: "/examples/label-text.jpg", label: "Text" },
];

const dataProviders = [
  { name: "TMDB", url: "https://www.themoviedb.org/", keyUrl: "https://www.themoviedb.org/settings/api", desc: "Movie & TV metadata and poster images" },
  { name: "MDBList", url: "https://mdblist.com/", keyUrl: "https://mdblist.com/preferences/", desc: "Aggregated ratings from multiple sources" },
  { name: "OMDb", url: "https://www.omdbapi.com/", keyUrl: "https://www.omdbapi.com/apikey.aspx", desc: "Alternative ratings source" },
  { name: "Fanart.tv", url: "https://fanart.tv/", keyUrl: "https://fanart.tv/get-an-api-key/", desc: "Fan art, logos, and backdrops" },
  { name: "RPDB", url: "https://ratingposterdb.com/", desc: "The original inspiration for this project" },
];

const ratingSources = [
  { name: "IMDb", url: "https://www.imdb.com/" },
  { name: "Rotten Tomatoes", url: "https://www.rottentomatoes.com/" },
  { name: "Metacritic", url: "https://www.metacritic.com/" },
  { name: "Trakt", url: "https://trakt.tv/" },
  { name: "Letterboxd", url: "https://letterboxd.com/" },
  { name: "MyAnimeList", url: "https://myanimelist.net/" },
];

const logos = [
  { src: "/examples/logo-nosferatu.png", label: "Nosferatu (1922)" },
  { src: "/examples/logo-metropolis.png", label: "Metropolis (1927)" },
  { src: "/examples/logo-caligari.png", alt: "The Cabinet of Dr. Caligari (1920)", label: "Dr. Caligari (1920)" },
  { src: "/examples/logo-phantom.png", alt: "The Phantom of the Opera (1925)", label: "Phantom of the Opera (1925)" },
  { src: "/examples/logo-trip-to-moon.png", alt: "A Trip to the Moon (1902)", label: "A Trip to the Moon (1902)" },
  { src: "/examples/logo-safety-last.png", alt: "Safety Last! (1923)", label: "Safety Last! (1923)" },
  { src: "/examples/logo-the-general.png", label: "The General (1926)" },
];

const backdrops = [
  { src: "/examples/backdrop-nosferatu.jpg", label: "Nosferatu (1922)" },
  { src: "/examples/backdrop-metropolis.jpg", label: "Metropolis (1927)" },
  { src: "/examples/backdrop-trip-to-moon.jpg", alt: "A Trip to the Moon (1902)", label: "A Trip to the Moon (1902)" },
];
</script>

<template>
  <div class="min-h-screen flex flex-col">
    <main class="flex-1 flex flex-col items-center px-4 py-16">
      <div class="max-w-5xl w-full space-y-20">
        <!-- Hero -->
        <div class="text-center space-y-3">
          <h1 class="text-4xl font-bold tracking-tight sm:text-5xl">OpenPosterDB</h1>
          <p class="text-xs text-muted-foreground">v{{ version }}</p>
          <p class="text-lg text-muted-foreground max-w-xl mx-auto">
            Self-hosted poster, logo, and backdrop serving for your media server.
          </p>
          <div class="pt-4">
            <NavButtons />
          </div>
          <div class="pt-4 max-w-lg mx-auto">
            <FreeApiKeyCard />
          </div>
          <!-- Feature cards -->
          <div class="pt-4 grid gap-4 sm:grid-cols-2 lg:grid-cols-4 text-left">
            <Card v-for="f in features" :key="f.title">
              <CardHeader class="pb-0">
                <CardTitle class="text-sm flex items-center gap-2">
                  <component :is="f.icon" class="h-4 w-4 text-muted-foreground shrink-0" />
                  {{ f.title }}
                </CardTitle>
              </CardHeader>
              <CardContent>
                <p class="text-sm text-muted-foreground">{{ f.desc }}</p>
              </CardContent>
            </Card>
          </div>
        </div>

        <!-- Real poster examples -->
        <div class="space-y-4 text-center">
          <h2 class="text-2xl font-semibold">Rating Badges</h2>
          <p class="text-sm text-muted-foreground max-w-lg mx-auto">
            Rating badges from IMDb, Letterboxd, Rotten Tomatoes, and more are overlaid directly
            onto your media posters.
          </p>
          <div class="flex flex-wrap justify-center gap-4">
            <div v-for="p in posters" :key="p.src" class="space-y-1">
              <BlurImage
                :src="p.src"
                :alt="p.alt || p.label"
                :width="160"
                :height="240"
                class="rounded-lg shadow-xl"
              />
              <p class="text-xs text-muted-foreground">{{ p.label }}</p>
            </div>
          </div>
          <p class="text-xs text-muted-foreground italic">
            All films shown are in the public domain
          </p>
        </div>

        <!-- Logos -->
        <div class="space-y-4 text-center">
          <h2 class="text-2xl font-semibold">Logos</h2>
          <p class="text-sm text-muted-foreground max-w-lg mx-auto">
            Serve transparent logos with rating badges attached below.
          </p>
          <div class="flex flex-wrap items-end justify-center gap-6">
            <div v-for="l in logos" :key="l.src" class="space-y-2">
              <div class="rounded-lg shadow-md bg-neutral-900 p-3">
                <BlurImage
                  :src="l.src"
                  :alt="l.alt || l.label"
                  :width="200"
                  :height="122"
                />
              </div>
              <p class="text-xs text-muted-foreground">{{ l.label }}</p>
            </div>
          </div>
        </div>

        <!-- Backdrops -->
        <div class="space-y-4 text-center">
          <h2 class="text-2xl font-semibold">Backdrops</h2>
          <p class="text-sm text-muted-foreground max-w-lg mx-auto">
            Backdrops get rating badges in the top-right corner for a clean overlay.
          </p>
          <div class="flex flex-wrap justify-center gap-4">
            <div v-for="b in backdrops" :key="b.src" class="space-y-1">
              <BlurImage
                :src="b.src"
                :alt="b.alt || b.label"
                :width="360"
                :height="203"
                class="rounded-lg shadow-md"
              />
              <p class="text-xs text-muted-foreground">{{ b.label }}</p>
            </div>
          </div>
        </div>

        <!-- Badge Position -->
        <div class="space-y-4 text-center">
          <h2 class="text-2xl font-semibold">Badge Position</h2>
          <p class="text-sm text-muted-foreground max-w-lg mx-auto">
            Place rating badges anywhere on the poster &mdash; corners, edges, or centered.
          </p>
          <div class="grid grid-cols-2 sm:grid-cols-4 gap-4 justify-items-center">
            <div v-for="p in positions" :key="p.src" class="space-y-2">
              <BlurImage
                :src="p.src"
                :alt="p.label"
                :width="160"
                :height="240"
                class="rounded-lg shadow-md"
              />
              <p class="text-xs text-muted-foreground">{{ p.label }}</p>
            </div>
          </div>
        </div>

        <!-- Badge Style -->
        <div class="space-y-4 text-center">
          <h2 class="text-2xl font-semibold">Badge Style</h2>
          <p class="text-sm text-muted-foreground max-w-lg mx-auto">
            Horizontal badges show the source icon and score side by side. Vertical badges stack
            them.
          </p>
          <div class="flex flex-wrap items-end justify-center gap-6">
            <div v-for="s in styles" :key="s.src" class="space-y-2">
              <BlurImage
                :src="s.src"
                :alt="s.label"
                :width="200"
                :height="300"
                class="rounded-lg shadow-md"
              />
              <p class="text-xs text-muted-foreground">{{ s.label }}</p>
            </div>
          </div>
        </div>

        <!-- Label Style -->
        <div class="space-y-4 text-center">
          <h2 class="text-2xl font-semibold">Label Style</h2>
          <p class="text-sm text-muted-foreground max-w-lg mx-auto">
            Show rating sources as colored icons or as text labels.
          </p>
          <div class="flex flex-wrap items-end justify-center gap-6">
            <div v-for="l in labels" :key="l.src" class="space-y-2">
              <BlurImage
                :src="l.src"
                :alt="l.label"
                :width="200"
                :height="300"
                class="rounded-lg shadow-md"
              />
              <p class="text-xs text-muted-foreground">{{ l.label }}</p>
            </div>
          </div>
        </div>

        <!-- Acknowledgments -->
        <div class="space-y-6 text-center">
          <h2 class="text-2xl font-semibold">Acknowledgments</h2>
          <p class="text-sm text-muted-foreground max-w-lg mx-auto">
            OpenPosterDB is made possible by these third-party services.
          </p>
          <div class="flex flex-wrap justify-center gap-3 text-left max-w-3xl mx-auto">
            <div
              v-for="p in dataProviders"
              :key="p.name"
              class="rounded-lg border p-3 w-full sm:w-[calc(50%-0.375rem)] lg:w-[calc(33.333%-0.5rem)]"
            >
              <p class="text-sm font-medium">{{ p.name }}</p>
              <p class="text-xs text-muted-foreground mb-2">{{ p.desc }}</p>
              <div class="flex gap-3">
                <a
                  :href="p.url"
                  target="_blank"
                  rel="noopener noreferrer"
                  class="text-xs text-muted-foreground hover:text-foreground transition-colors inline-flex items-center gap-1"
                >
                  <ExternalLink class="h-3 w-3" />
                  Homepage
                </a>
                <a
                  v-if="p.keyUrl"
                  :href="p.keyUrl"
                  target="_blank"
                  rel="noopener noreferrer"
                  class="text-xs text-muted-foreground hover:text-foreground transition-colors inline-flex items-center gap-1"
                >
                  <KeyRound class="h-3 w-3" />
                  Get API key
                </a>
              </div>
            </div>
          </div>
          <div class="pt-2">
            <p class="text-xs text-muted-foreground mb-2">Rating sources</p>
            <div class="flex flex-wrap justify-center gap-x-4 gap-y-1">
              <a
                v-for="r in ratingSources"
                :key="r.name"
                :href="r.url"
                target="_blank"
                rel="noopener noreferrer"
                class="text-xs text-muted-foreground hover:text-foreground transition-colors"
              >
                {{ r.name }}
              </a>
            </div>
          </div>
          <p class="text-xs text-muted-foreground italic max-w-lg mx-auto">
            This product uses the TMDB API but is not endorsed or certified by TMDB.
          </p>
        </div>

      </div>
    </main>
  </div>
</template>
