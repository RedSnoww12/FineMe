import { useState, type FormEvent } from 'react'
import { confirmationErrors, confirmationInput, type Confirmation, type ConfirmationErrors } from '../lib/confirmation'
import { Button, FormError, TextField } from '@/shared/ui/ui'

export function ConfirmationFields({
  form,
  errors,
  totpEnabled,
  onChange,
}: {
  form: Confirmation
  errors: ConfirmationErrors
  totpEnabled: boolean
  onChange: (form: Confirmation) => void
}) {
  return (
    <>
      <TextField
        label="Mot de passe"
        type="password"
        value={form.password}
        onChange={(password) => onChange({ ...form, password })}
        error={errors.password}
        autoComplete="current-password"
      />
      {totpEnabled ? (
        <TextField
          label="Code de vérification"
          value={form.code}
          onChange={(code) => onChange({ ...form, code })}
          error={errors.code}
          hint="Code de l’application ou code de secours."
          autoComplete="one-time-code"
        />
      ) : null}
    </>
  )
}

/** Révocation d'un accès IA, jeton ou connexion OAuth, confirmée avant envoi. */
export function RevokeAccessForm({
  label,
  totpEnabled,
  error,
  busy,
  onConfirm,
  onCancel,
}: {
  label: string
  totpEnabled: boolean
  error: unknown
  busy: boolean
  onConfirm: (input: { password: string; code?: string }) => void
  onCancel: () => void
}) {
  const [form, setForm] = useState<Confirmation>({ password: '', code: '' })
  const [errors, setErrors] = useState<ConfirmationErrors>({})

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const next = confirmationErrors(form, totpEnabled)
    setErrors(next)
    if (Object.keys(next).length === 0) onConfirm(confirmationInput(form))
  }

  return (
    <form onSubmit={submit} noValidate aria-label={label} className="mt-2 flex max-w-[420px] flex-col gap-3">
      <p className="text-[13.5px] text-muted">Confirmez pour couper l’accès de ce client immédiatement.</p>
      <ConfirmationFields form={form} errors={errors} totpEnabled={totpEnabled} onChange={setForm} />
      <FormError error={error} />
      <div className="flex flex-wrap gap-2">
        <Button size="sm" onClick={onCancel}>
          Annuler
        </Button>
        <Button size="sm" type="submit" variant="danger" busy={busy}>
          Révoquer
        </Button>
      </div>
    </form>
  )
}
