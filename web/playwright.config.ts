import { defineConfig, devices } from '@playwright/test'

// Les parcours E2E tournent contre une pile réelle (front + API + base jetable) déjà démarrée.
// E2E_BASE_URL : adresse du front, par défaut le serveur de développement Vite.
export default defineConfig({
  testDir: './e2e',
  // Les parcours partagent l'utilisateur créé par la première configuration : ordre imposé.
  fullyParallel: false,
  workers: 1,
  retries: 0,
  forbidOnly: Boolean(process.env.CI),
  reporter: [['list']],
  timeout: 60_000,
  use: {
    baseURL: process.env.E2E_BASE_URL ?? 'http://localhost:5173',
    locale: 'fr-FR',
    trace: 'retain-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
})
