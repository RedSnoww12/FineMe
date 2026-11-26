import { useState, type FormEvent } from 'react'
import { useAccessTokens, useCreateAccessToken, useMcpCalls, useRevokeAccessToken } from '../hooks/useAccessTokens'
import { useTotpStatus } from '../hooks/useTotp'
import {
  MAX_ACTIVE_TOKENS,
  activeCount,
  callsSummary,
  claudeAddCommand,
  isActive,
  mcpEndpoint,
  toolLabel,
} from '../lib/accessTokens'
import { apiBaseUrl } from '@/shared/api/client'
import type { AccessToken, CreatedAccessToken } from '@/shared/api/types'
import { dateTime } from '@/shared/lib/format'
import { Badge, Button, ErrorState, FormError, LoadingBlock, Notice, Panel, TextField } from '@/shared/ui/ui'

type Confirmation = { password: string; code: string }
type ConfirmationErrors = Partial<Record<keyof Confirmation | 'name', string>>

/** Erreurs de saisie de la confirmation (mot de passe, et code si le second facteur est activé). */
function confirmationErrors(form: Confirmation, totpEnabled: boolean): ConfirmationErrors {
  const errors: ConfirmationErrors = {}
  if (!form.password) errors.password = 'Saisissez votre mot de passe.'
  if (totpEnabled && !form.code.trim()) errors.code = 'Saisissez un code de l’application ou un code de secours.'
  return errors
}

function confirmationInput(form: Confirmation) {
  const code = form.code.replace(/\s+/g, '')
  return { password: form.password, ...(code ? { code } : {}) }
}

function ConfirmationFields({
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

/** Jeton tout juste créé : affiché une seule fois, avec la commande à lancer. */
function CreatedSecret({ created, onDone }: { created: CreatedAccessToken; onDone: () => void }) {
  const endpoint = mcpEndpoint(apiBaseUrl(), window.location.origin)
  const command = claudeAddCommand(endpoint, created.secret)
  const [copy, setCopy] = useState<'idle' | 'done' | 'failed'>('idle')
  const copyCommand = () => {
    // Presse-papiers indisponible (navigateur, permission) : la commande reste sélectionnable à l'écran.
    if (!navigator.clipboard) return setCopy('failed')
    navigator.clipboard.writeText(command).then(
      () => setCopy('done'),
      () => setCopy('failed'),
    )
  }
  return (
    <div className="flex flex-col gap-3">
      <Notice tone="pos">
        Jeton « {created.token.name} » créé. Copiez-le maintenant : il ne sera plus jamais affiché. Il donne accès en
        lecture à tout votre patrimoine, gardez-le comme un mot de passe.
      </Notice>
      <p className="text-muted">Dans un terminal, branchez Claude Code sur FineMe :</p>
      <pre
        aria-label="Commande Claude Code"
        className="overflow-x-auto whitespace-pre-wrap break-all rounded-[9px] bg-surface-2 px-3 py-2.5 font-mono text-[13px]"
      >
        {command}
      </pre>
      <div className="flex flex-wrap gap-2">
        <Button onClick={copyCommand}>{copy === 'done' ? 'Commande copiée' : 'Copier la commande'}</Button>
        <Button variant="primary" onClick={onDone}>
          J’ai copié le jeton
        </Button>
      </div>
      {copy === 'failed' ? (
        <p role="alert" className="text-[13.5px] text-neg">
          Copie impossible depuis ce navigateur : sélectionnez la commande ci-dessus et copiez-la.
        </p>
      ) : null}
    </div>
  )
}

function CreateForm({
  totpEnabled,
  full,
  onCreated,
}: {
  totpEnabled: boolean
  full: boolean
  onCreated: (c: CreatedAccessToken) => void
}) {
  const [name, setName] = useState('')
  const [form, setForm] = useState<Confirmation>({ password: '', code: '' })
  const [errors, setErrors] = useState<ConfirmationErrors>({})
  const create = useCreateAccessToken((created) => {
    setName('')
    setForm({ password: '', code: '' })
    onCreated(created)
  })

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const next = confirmationErrors(form, totpEnabled)
    if (!name.trim()) next.name = 'Donnez un nom au jeton (par exemple « Claude Code portable »).'
    setErrors(next)
    if (Object.keys(next).length === 0) create.mutate({ name: name.trim(), ...confirmationInput(form) })
  }

  if (full) {
    return <p className="text-muted">{MAX_ACTIVE_TOKENS} jetons actifs : révoquez-en un pour en créer un autre.</p>
  }
  return (
    <form
      onSubmit={submit}
      noValidate
      aria-label="Créer un jeton d’accès"
      className="flex max-w-[420px] flex-col gap-4"
    >
      <TextField
        label="Nom du jeton"
        value={name}
        onChange={setName}
        error={errors.name}
        hint="Pour le reconnaître dans la liste et le journal."
        maxLength={60}
        autoComplete="off"
      />
      <ConfirmationFields form={form} errors={errors} totpEnabled={totpEnabled} onChange={setForm} />
      <FormError error={create.error} />
      <div>
        <Button type="submit" variant="primary" busy={create.isPending}>
          Créer le jeton
        </Button>
      </div>
    </form>
  )
}

function RevokeForm({
  token,
  totpEnabled,
  onCancel,
}: {
  token: AccessToken
  totpEnabled: boolean
  onCancel: () => void
}) {
  const [form, setForm] = useState<Confirmation>({ password: '', code: '' })
  const [errors, setErrors] = useState<ConfirmationErrors>({})
  const revoke = useRevokeAccessToken(onCancel)

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const next = confirmationErrors(form, totpEnabled)
    setErrors(next)
    if (Object.keys(next).length === 0) revoke.mutate({ id: token.id, ...confirmationInput(form) })
  }

  return (
    <form
      onSubmit={submit}
      noValidate
      aria-label={`Révoquer ${token.name}`}
      className="mt-2 flex max-w-[420px] flex-col gap-3"
    >
      <p className="text-[13.5px] text-muted">Confirmez pour couper l’accès de ce client immédiatement.</p>
      <ConfirmationFields form={form} errors={errors} totpEnabled={totpEnabled} onChange={setForm} />
      <FormError error={revoke.error} />
      <div className="flex flex-wrap gap-2">
        <Button size="sm" onClick={onCancel}>
          Annuler
        </Button>
        <Button size="sm" type="submit" variant="danger" busy={revoke.isPending}>
          Révoquer
        </Button>
      </div>
    </form>
  )
}

function TokenList({ tokens, totpEnabled }: { tokens: AccessToken[]; totpEnabled: boolean }) {
  const [revoking, setRevoking] = useState<string | null>(null)
  if (tokens.length === 0) return <p className="text-muted">Aucun jeton pour le moment.</p>
  return (
    <ul aria-label="Jetons d’accès" className="flex flex-col divide-y divide-line">
      {tokens.map((token) => (
        <li key={token.id} className="py-2.5">
          <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1">
            <div className="flex min-w-0 flex-col">
              <span className="font-medium">{token.name}</span>
              <span className="text-[13.5px] text-muted">
                <span className="font-mono">{token.hint}</span> · créé le {dateTime(token.created_at)} ·{' '}
                {token.last_used_at ? `utilisé le ${dateTime(token.last_used_at)}` : 'jamais utilisé'}
              </span>
            </div>
            <div className="flex items-center gap-2.5">
              <Badge tone={isActive(token) ? 'pos' : 'muted'}>{isActive(token) ? 'Actif' : 'Révoqué'}</Badge>
              {isActive(token) && revoking !== token.id ? (
                <Button
                  size="sm"
                  variant="danger"
                  onClick={() => setRevoking(token.id)}
                  aria-label={`Révoquer ${token.name}`}
                >
                  Révoquer
                </Button>
              ) : null}
            </div>
          </div>
          {revoking === token.id ? (
            <RevokeForm token={token} totpEnabled={totpEnabled} onCancel={() => setRevoking(null)} />
          ) : null}
        </li>
      ))}
    </ul>
  )
}

function CallJournal() {
  const calls = useMcpCalls()
  if (calls.isPending) return <LoadingBlock rows={2} label="Chargement du journal…" />
  if (calls.isError) return <ErrorState error={calls.error} onRetry={() => void calls.refetch()} />
  if (calls.data.length === 0) return <p className="text-muted">Aucun appel pour le moment.</p>
  return (
    <div className="flex flex-col gap-2">
      <p className="text-muted">{callsSummary(calls.data)} récents.</p>
      <ul aria-label="Appels récents" className="flex flex-col divide-y divide-line">
        {calls.data.map((call, index) => (
          <li key={`${call.at}-${call.token_id}-${call.tool}-${index}`} className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1 py-2">
            <span>
              <span className="font-medium">{toolLabel(call.tool)}</span>
              <span className="text-[13.5px] text-muted"> · {call.token_name}</span>
            </span>
            <span className="flex items-center gap-2.5">
              <time dateTime={call.at} className="text-[13.5px] text-muted">
                {dateTime(call.at)}
              </time>
              <Badge tone={call.success ? 'pos' : 'neg'}>{call.success ? 'Réussi' : 'Erreur'}</Badge>
            </span>
          </li>
        ))}
      </ul>
    </div>
  )
}

/** Accès IA : jetons en lecture seule pour brancher un client IA (Claude) sur le serveur MCP, et journal des appels. */
export function AiAccessPanel() {
  const tokens = useAccessTokens()
  const totp = useTotpStatus()
  const [created, setCreated] = useState<CreatedAccessToken | null>(null)
  const totpEnabled = totp.data?.enabled ?? false

  return (
    <Panel title="Accès IA">
      <div className="flex flex-col gap-5">
        <p className="max-w-[64ch] text-muted">
          Un jeton d’accès permet à un assistant IA (Claude Code, Claude Desktop) de consulter votre patrimoine, en
          lecture seule : il ne peut rien modifier ni ouvrir le reste de l’application. Changer votre mot de passe
          révoque tous les jetons.
        </p>
        {tokens.isPending ? (
          <LoadingBlock rows={2} label="Chargement des jetons…" />
        ) : tokens.isError ? (
          <ErrorState error={tokens.error} onRetry={() => void tokens.refetch()} />
        ) : (
          <>
            {created ? (
              <CreatedSecret created={created} onDone={() => setCreated(null)} />
            ) : (
              <CreateForm
                totpEnabled={totpEnabled}
                full={activeCount(tokens.data) >= MAX_ACTIVE_TOKENS}
                onCreated={setCreated}
              />
            )}
            <section aria-label="Jetons">
              <h3 className="mb-1 font-medium">Jetons</h3>
              <TokenList tokens={tokens.data} totpEnabled={totpEnabled} />
            </section>
          </>
        )}
        <section aria-label="Journal des appels">
          <h3 className="mb-1 font-medium">Journal des appels</h3>
          <CallJournal />
        </section>
      </div>
    </Panel>
  )
}
