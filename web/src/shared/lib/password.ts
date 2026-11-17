/** Longueur minimale imposée par l'API (docs/API.md), comptée en caractères et non en unités UTF-16. */
export const MIN_PASSWORD_LENGTH = 10

export type NewPasswordErrors = { password?: string; confirm?: string }

/** Erreurs de saisie d'un nouveau mot de passe et de sa confirmation ; objet vide = saisie valide. */
export function newPasswordErrors(password: string, confirm: string): NewPasswordErrors {
  const errors: NewPasswordErrors = {}
  if ([...password].length < MIN_PASSWORD_LENGTH) {
    errors.password = `Le mot de passe doit contenir au moins ${MIN_PASSWORD_LENGTH} caractères.`
  }
  if (confirm !== password) errors.confirm = 'Les deux mots de passe ne correspondent pas.'
  return errors
}
