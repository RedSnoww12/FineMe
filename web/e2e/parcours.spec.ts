import { expect, test, type Page } from '@playwright/test'

// Parcours critiques, contre une pile réelle dont la base est VIDE (aucun utilisateur).
// L'utilisateur est fictif et unique à chaque exécution ; rien n'est lu ni écrit ailleurs.
const run = Date.now()
const account = {
  name: 'Camille Essai',
  email: `e2e-${run}@exemple.test`,
  password: `essai-${run}-tournesol`,
}

test.describe.configure({ mode: 'serial' })

test.beforeAll(async ({ request }) => {
  const response = await request.get('/api/auth/status')
  expect(response.ok(), 'L’API doit répondre sur /api/auth/status').toBeTruthy()
  const status = (await response.json()) as { setup_required: boolean }
  expect(
    status.setup_required,
    'Ces parcours exigent une base vide : un utilisateur existe déjà sur cette pile. Lancez-les contre une pile jetable.',
  ).toBe(true)
})

async function signIn(page: Page) {
  await page.goto('/connexion')
  await expect(page.getByRole('heading', { name: 'Connexion' })).toBeVisible()
  await page.getByLabel('Adresse e-mail').fill(account.email)
  await page.getByLabel('Mot de passe', { exact: true }).fill(account.password)
  await page.getByRole('button', { name: 'Se connecter' }).click()
  // Après connexion, l'application revient sur la dernière page visitée : on attend le shell, pas un écran précis.
  await expect(page.getByRole('navigation', { name: 'Navigation principale' })).toBeVisible()
}

async function addAccount(page: Page, name: string, balance: string) {
  await page.goto('/comptes')
  await page.getByRole('button', { name: 'Ajouter un compte' }).first().click()
  const dialog = page.getByRole('dialog', { name: 'Ajouter un compte' })
  await dialog.getByLabel('Nom du compte').fill(name)
  await dialog.getByLabel('Solde espèces').fill(balance)
  await dialog.getByRole('button', { name: 'Ajouter', exact: true }).click()
  await expect(dialog).toBeHidden()
  await expect(page.getByRole('link', { name: new RegExp(name) })).toBeVisible()
}

/** Montant affiché sous « Patrimoine net », espaces insécables normalisées. */
async function netWorth(page: Page): Promise<string> {
  await page.goto('/')
  const figure = page.locator('section', { hasText: 'Patrimoine net' }).first().locator('p').nth(1)
  await expect(figure).toBeVisible()
  return (await figure.innerText()).replace(/[  ]/g, ' ')
}

test('première configuration, déconnexion puis connexion', async ({ page }) => {
  await page.goto('/')
  await expect(page).toHaveURL(/\/connexion$/)
  await expect(page.getByRole('heading', { name: 'Première configuration' })).toBeVisible()

  await page.getByLabel('Votre nom').fill(account.name)
  await page.getByLabel('Adresse e-mail').fill(account.email)
  await page.getByLabel('Mot de passe', { exact: true }).fill(account.password)
  await page.getByLabel('Confirmez le mot de passe').fill(account.password)
  await page.getByRole('button', { name: 'Créer mon compte' }).click()

  await expect(page.getByRole('heading', { name: 'Vue d’ensemble' })).toBeVisible()
  await expect(page.getByRole('heading', { name: 'Aucun compte pour l’instant' })).toBeVisible()

  await page.goto('/parametres')
  await expect(page.getByText(account.email)).toBeVisible()
  await page.getByRole('button', { name: 'Se déconnecter' }).click()
  await expect(page.getByRole('heading', { name: 'Connexion' })).toBeVisible()

  await signIn(page)
})

test('créer un compte fait évoluer le patrimoine net', async ({ page }) => {
  await signIn(page)

  await addAccount(page, 'Compte courant essai', '1500')
  expect(await netWorth(page)).toBe('1 500 €')

  await addAccount(page, 'Livret essai', '2500')
  expect(await netWorth(page)).toBe('4 000 €')
})

test('le simulateur de frais compare deux scénarios', async ({ page }) => {
  await signIn(page)
  await page.goto('/frais')
  await expect(page.getByRole('heading', { name: 'Simulateur de frais' })).toBeVisible()
  await expect(page.getByText('Saisissez un montant initial ou un versement mensuel')).toBeVisible()

  await page.getByLabel('Montant initial').fill('10000')
  await page.getByLabel('Versement mensuel').fill('200')
  await expect(page.getByText('Saisissez un montant initial ou un versement mensuel')).toBeHidden()

  const firstScenario = page.locator('section', { hasText: 'Scénario 1' }).first()
  const finalValue = firstScenario.getByText('Valeur finale', { exact: true }).locator('..')
  const before = await finalValue.innerText()
  expect(before).toMatch(/\d/)

  await firstScenario.getByLabel('Frais courants des fonds').fill('2')
  await expect(finalValue).not.toHaveText(before)
  await expect(page.getByText('La plus élevée')).toHaveCount(1)
})
