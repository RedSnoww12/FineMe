// Confirmation d'une action sensible : mot de passe, et code si le second facteur est activé.

export type Confirmation = { password: string; code: string }
export type ConfirmationErrors = Partial<Record<keyof Confirmation | 'name', string>>

/** Erreurs de saisie de la confirmation. */
export function confirmationErrors(form: Confirmation, totpEnabled: boolean): ConfirmationErrors {
  const errors: ConfirmationErrors = {}
  if (!form.password) errors.password = 'Saisissez votre mot de passe.'
  if (totpEnabled && !form.code.trim()) errors.code = 'Saisissez un code de l’application ou un code de secours.'
  return errors
}

/** Corps envoyé à l'API : le code sans espaces, omis s'il est vide. */
export function confirmationInput(form: Confirmation): { password: string; code?: string } {
  const code = form.code.replace(/\s+/g, '')
  return { password: form.password, ...(code ? { code } : {}) }
}
