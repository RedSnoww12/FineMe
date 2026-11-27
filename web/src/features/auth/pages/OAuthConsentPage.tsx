import { useState, type FormEvent } from 'react'
import { Link, useSearchParams } from 'react-router'
import { ThrottledSubmit } from '../components/ThrottledSubmit'
import { useApproveRequest, useConsentRequest, useDenyRequest } from '../hooks/useConsentApi'
import { approvalInput, expiryTime, isRequestId } from '../lib/consent'
import { ApiError } from '@/shared/api/client'
import type { OAuthRequestView } from '@/shared/api/types'
import { Logo } from '@/shared/ui/Icon'
import { Button, ErrorState, FormError, LoadingBlock, TextField } from '@/shared/ui/ui'

function Expired() {
  return (
    <div className="flex flex-col gap-3">
      <h1 className="text-[24px] font-semibold">Demande introuvable ou expirée</h1>
      <p className="text-muted">
        Une demande d’autorisation ne sert qu’une fois et expire au bout de quelques minutes. Relancez la connexion
        depuis votre assistant.
      </p>
      <Link className="font-medium text-accent underline" to="/">
        Revenir à FineMe
      </Link>
    </div>
  )
}

/** La demande n'existe plus : expirée, ou déjà acceptée ou refusée ailleurs. */
function isGone(error: unknown): boolean {
  return error instanceof ApiError && error.status === 404
}

function ConsentForm({ id, request, readAt }: { id: string; request: OAuthRequestView; readAt: number }) {
  const [password, setPassword] = useState('')
  const [code, setCode] = useState('')
  const [missing, setMissing] = useState<string>()
  const approve = useApproveRequest(id, request.redirect_host)
  const deny = useDenyRequest(id, request.redirect_host)
  // Après la réponse, le navigateur part vers le client : plus rien à envoyer.
  const leaving = approve.isSuccess || deny.isSuccess

  const submit = (event: FormEvent) => {
    event.preventDefault()
    setMissing(password ? undefined : 'Saisissez votre mot de passe.')
    if (!password) return
    approve.mutate(approvalInput(password, code), {
      // Rien de saisi ne reste dans la page une fois l'accord donné.
      onSuccess: () => {
        setPassword('')
        setCode('')
      },
    })
  }

  if (isGone(approve.error) || isGone(deny.error)) return <Expired />

  return (
    <form onSubmit={submit} noValidate aria-label="Autoriser l’accès" className="flex flex-col gap-4">
      <h1 className="text-[24px] font-semibold break-words">Autoriser « {request.client_name} » ?</h1>
      <dl className="flex flex-col gap-2 rounded-[9px] bg-surface-2 px-3.5 py-3 text-[14px]">
        <div>
          <dt className="text-muted">L’accès sera envoyé à</dt>
          <dd className="font-mono font-medium break-all">{request.redirect_host}</dd>
        </div>
        <div>
          <dt className="text-muted">Portée</dt>
          <dd>Lecture seule de tout le patrimoine du foyer : rien ne peut être modifié.</dd>
        </div>
      </dl>
      <p className="text-[14px] text-muted">
        N’autorisez que si vous venez de lancer vous-même cette connexion et reconnaissez l’adresse ci-dessus. Sinon,
        refusez. Le nom affiché est choisi par le demandeur.
      </p>
      <TextField
        label="Mot de passe"
        type="password"
        value={password}
        onChange={setPassword}
        error={missing}
        autoComplete="current-password"
      />
      <TextField
        label="Code de vérification"
        value={code}
        onChange={setCode}
        hint="Seulement si la vérification en deux étapes est activée : code de l’application ou code de secours."
        autoComplete="one-time-code"
      />
      <ThrottledSubmit
        error={approve.error}
        submittedAt={approve.submittedAt}
        busy={approve.isPending}
        disabled={leaving || deny.isPending}
      >
        Autoriser
      </ThrottledSubmit>
      <FormError error={deny.error} />
      <Button busy={deny.isPending} disabled={leaving || approve.isPending} onClick={() => deny.mutate()}>
        Refuser
      </Button>
      <p className="text-[13px] text-muted">
        Cette demande expire à {expiryTime(readAt, request.expires_in)}. L’accès se révoque à tout moment dans
        Paramètres, rubrique Accès IA.
      </p>
    </form>
  )
}

function ConsentContent({ id }: { id: string }) {
  const valid = isRequestId(id)
  const request = useConsentRequest(id, valid)
  if (!valid) return <Expired />
  if (request.data) return <ConsentForm id={id} request={request.data} readAt={request.dataUpdatedAt} />
  if (request.isPending) return <LoadingBlock label="Chargement de la demande…" />
  if (isGone(request.error)) return <Expired />
  return <ErrorState error={request.error} onRetry={() => void request.refetch()} />
}

/** Écran de consentement du connecteur OAuth : public, confirmé par le mot de passe et le second facteur. */
export default function OAuthConsentPage() {
  const [params] = useSearchParams()
  return (
    <main className="mx-auto flex min-h-dvh w-full max-w-[460px] flex-col justify-center gap-6 px-5 py-10">
      <div className="flex items-center gap-3">
        <Logo size={32} />
        <span className="font-display text-[22px] font-semibold tracking-tight">FineMe</span>
      </div>
      <div className="panel p-5 sm:p-7">
        <ConsentContent id={params.get('request') ?? ''} />
      </div>
    </main>
  )
}
