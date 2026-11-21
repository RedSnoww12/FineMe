import { useState, type FormEvent } from 'react'
import {
  useDisableTotp,
  useEnableTotp,
  useRegenerateRecoveryCodes,
  useStartTotp,
  useTotpStatus,
} from '../hooks/useTotp'
import { downloadFile } from '../lib/download'
import { groupSecret, qrDataUri, recoveryCodesText } from '../lib/totp'
import type { TotpEnrollment } from '@/shared/api/types'
import { Badge, Button, ErrorState, FormError, LoadingBlock, Notice, Panel, TextField } from '@/shared/ui/ui'
import { useAuth } from '@/shared/state/auth'

const CODE_REQUIRED = 'Saisissez le code de votre application.'

/** Codes de secours à conserver : affichés une seule fois par le serveur. */
function RecoveryCodes({ codes, onDone }: { codes: string[]; onDone: () => void }) {
  return (
    <div className="flex flex-col gap-3">
      <Notice tone="pos">
        Conservez ces codes de secours en lieu sûr : chacun permet une connexion sans téléphone, une seule fois. Ils ne
        seront plus affichés.
      </Notice>
      <ul aria-label="Codes de secours" className="grid grid-cols-2 gap-2 font-mono text-[14.5px] sm:max-w-[360px]">
        {codes.map((code) => (
          <li key={code} className="rounded-[7px] bg-surface-2 px-2.5 py-1.5 text-center">
            {code}
          </li>
        ))}
      </ul>
      <div className="flex flex-wrap gap-2">
        <Button onClick={() => downloadFile(recoveryCodesText(codes), 'fineme-codes-de-secours.txt', 'text/plain')}>
          Télécharger
        </Button>
        <Button variant="primary" onClick={onDone}>
          J’ai noté mes codes
        </Button>
      </div>
    </div>
  )
}

/** Étape d'activation : QR code (ou secret à saisir) puis confirmation par un premier code. */
function Enrollment({ enrollment, onEnabled }: { enrollment: TotpEnrollment; onEnabled: (codes: string[]) => void }) {
  const { renewSession } = useAuth()
  const [code, setCode] = useState('')
  const [error, setError] = useState<string>()
  const mutation = useEnableTotp((activation) => {
    // Les autres appareils sont déconnectés : ce nouveau jeton garde celui-ci connecté.
    renewSession(activation)
    onEnabled(activation.recovery_codes)
  })

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const cleaned = code.replace(/\s+/g, '')
    setError(cleaned ? undefined : CODE_REQUIRED)
    if (cleaned) mutation.mutate(cleaned)
  }

  return (
    <form onSubmit={submit} noValidate className="flex max-w-[420px] flex-col gap-4">
      <p className="text-muted">
        Scannez ce QR code avec votre application d’authentification (Aegis, Google Authenticator, 1Password…), puis
        saisissez le code à 6 chiffres qu’elle affiche.
      </p>
      <img
        src={qrDataUri(enrollment.qr_svg)}
        alt="QR code à scanner avec l’application d’authentification"
        width={200}
        height={200}
        className="rounded-[9px] bg-white p-2"
      />
      <p className="text-[13.5px] text-muted">
        Saisie manuelle : <span className="font-mono text-ink">{groupSecret(enrollment.secret)}</span>
      </p>
      <TextField
        label="Code de l’application"
        value={code}
        onChange={setCode}
        error={error}
        autoComplete="one-time-code"
        inputMode="numeric"
      />
      <FormError error={mutation.error} />
      <div>
        <Button type="submit" variant="primary" busy={mutation.isPending}>
          Activer
        </Button>
      </div>
    </form>
  )
}

/** Désactivation (mot de passe + code) et régénération des codes de secours. */
function EnabledActions({ remaining, onCodes }: { remaining: number; onCodes: (codes: string[]) => void }) {
  const { renewSession } = useAuth()
  const [form, setForm] = useState({ password: '', code: '' })
  const [errors, setErrors] = useState<Partial<Record<keyof typeof form, string>>>({})
  const disable = useDisableTotp((result) => {
    renewSession(result)
    setForm({ password: '', code: '' })
  })
  const regenerate = useRegenerateRecoveryCodes((codes) => {
    setForm({ password: '', code: '' })
    onCodes(codes)
  })
  const set = (key: keyof typeof form) => (value: string) => setForm((f) => ({ ...f, [key]: value }))
  const code = form.code.replace(/\s+/g, '')

  const submitDisable = (event: FormEvent) => {
    event.preventDefault()
    const next: typeof errors = {}
    if (!form.password) next.password = 'Saisissez votre mot de passe.'
    if (!code) next.code = 'Saisissez un code de l’application ou un code de secours.'
    setErrors(next)
    if (Object.keys(next).length === 0) disable.mutate({ password: form.password, code })
  }

  const submitRegenerate = () => {
    setErrors(code ? {} : { code: CODE_REQUIRED })
    if (code) regenerate.mutate(code)
  }

  return (
    <form onSubmit={submitDisable} noValidate className="flex max-w-[420px] flex-col gap-4">
      <p className="text-muted">
        {remaining} code{remaining > 1 ? 's' : ''} de secours restant{remaining > 1 ? 's' : ''}. Pour désactiver la
        vérification, confirmez avec votre mot de passe et un code.
      </p>
      <TextField
        label="Mot de passe"
        type="password"
        value={form.password}
        onChange={set('password')}
        error={errors.password}
        autoComplete="current-password"
      />
      <TextField
        label="Code de vérification"
        value={form.code}
        onChange={set('code')}
        error={errors.code}
        hint="Code de l’application (ou code de secours pour désactiver)."
        autoComplete="one-time-code"
      />
      <FormError error={disable.error ?? regenerate.error} />
      <div className="flex flex-wrap gap-2">
        <Button onClick={submitRegenerate} busy={regenerate.isPending}>
          Régénérer les codes de secours
        </Button>
        <Button type="submit" variant="danger" busy={disable.isPending}>
          Désactiver
        </Button>
      </div>
    </form>
  )
}

/** Second facteur d'authentification (TOTP) : état, activation, codes de secours, désactivation. */
export function TotpPanel() {
  const status = useTotpStatus()
  const start = useStartTotp()
  const [codes, setCodes] = useState<string[] | null>(null)

  const badge = status.data ? (
    <Badge tone={status.data.enabled ? 'pos' : 'muted'}>{status.data.enabled ? 'Activée' : 'Désactivée'}</Badge>
  ) : null

  return (
    <Panel title="Vérification en deux étapes" action={badge}>
      {status.isPending ? (
        <LoadingBlock rows={2} label="Chargement de la vérification en deux étapes…" />
      ) : status.isError ? (
        <ErrorState error={status.error} onRetry={() => void status.refetch()} />
      ) : codes ? (
        <RecoveryCodes codes={codes} onDone={() => setCodes(null)} />
      ) : status.data.enabled ? (
        <EnabledActions remaining={status.data.recovery_codes_remaining} onCodes={setCodes} />
      ) : start.data ? (
        <Enrollment
          enrollment={start.data}
          onEnabled={(recovery) => {
            start.reset()
            setCodes(recovery)
          }}
        />
      ) : (
        <div className="flex max-w-[420px] flex-col gap-4">
          <p className="text-muted">
            Ajoutez un code à 6 chiffres, généré par une application de votre téléphone, en plus du mot de passe. Les
            autres appareils devront se reconnecter.
          </p>
          <FormError error={start.error} />
          <div>
            <Button variant="primary" busy={start.isPending} onClick={() => start.mutate()}>
              Activer la vérification en deux étapes
            </Button>
          </div>
        </div>
      )}
    </Panel>
  )
}
