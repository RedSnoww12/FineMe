/// <reference types="vitest/config" />
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'
import { VitePWA } from 'vite-plugin-pwa'

/**
 * Application installable. Le service worker ne précache que la coquille statique (HTML, JS, CSS, polices, icônes)
 * et ne met jamais en cache les réponses de l'API : elles sont authentifiées et personnelles. L'affichage hors ligne
 * des dernières données consultées passe par la persistance du cache TanStack Query, purgée à la déconnexion.
 */
const pwa = VitePWA({
  registerType: 'autoUpdate',
  injectRegister: 'script-defer',
  disable: Boolean(process.env.VITEST),
  // Les écrans de démarrage iOS ne servent qu'à l'installation : inutile de les précacher.
  includeAssets: ['favicon.svg', 'apple-touch-icon.png'],
  manifest: {
    id: '/',
    name: 'FineMe',
    short_name: 'FineMe',
    description: 'Le suivi du patrimoine de votre foyer.',
    lang: 'fr',
    dir: 'ltr',
    start_url: '/',
    scope: '/',
    display: 'standalone',
    orientation: 'portrait',
    background_color: '#EDF0F3',
    theme_color: '#EDF0F3',
    categories: ['finance'],
    icons: [
      { src: 'favicon.svg', sizes: 'any', type: 'image/svg+xml', purpose: 'any' },
      { src: 'pwa-192x192.png', sizes: '192x192', type: 'image/png', purpose: 'any' },
      { src: 'pwa-512x512.png', sizes: '512x512', type: 'image/png', purpose: 'any' },
      { src: 'maskable-512x512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
    ],
  },
  workbox: {
    globPatterns: ['**/*.{js,css,html,svg,woff2,ico}', 'pwa-*.png', 'apple-touch-icon.png'],
    navigateFallback: '/index.html',
    // Les appels à l'API (même origine via proxy ou réécriture) ne sont jamais servis par la coquille.
    navigateFallbackDenylist: [/^\/api\//, /^\/mcp/],
    runtimeCaching: [],
    cleanupOutdatedCaches: true,
  },
})

export default defineConfig({
  plugins: [react(), tailwindcss(), pwa],
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  server: {
    port: 5173,
    proxy: {
      '/api': { target: process.env.API_PROXY_TARGET ?? 'http://localhost:8080', changeOrigin: true },
      // Serveur MCP (client IA) : même cible que l'API.
      '/mcp': { target: process.env.API_PROXY_TARGET ?? 'http://localhost:8080', changeOrigin: true },
    },
  },
  test: {
    // Pyramide : unitaires (node, *.test.ts) et intégration (jsdom + MSW, *.test.tsx).
    projects: [
      {
        extends: true,
        test: { name: 'unit', environment: 'node', include: ['src/**/*.test.ts'] },
      },
      {
        extends: true,
        test: {
          name: 'integration',
          environment: 'jsdom',
          include: ['src/**/*.test.tsx'],
          setupFiles: ['src/test/setup.ts'],
          env: { VITE_API_URL: 'http://api.test' },
          testTimeout: 15_000,
        },
      },
    ],
  },

})
