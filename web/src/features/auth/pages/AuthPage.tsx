import { useState, type FormEvent } from 'react'
import { Navigate, useLocation } from 'react-router'
import { useAuthStatus, useLogin, useSetup } from '../hooks/useAuthApi'
import { Logo } from '@/shared/ui/Icon'
import { Button, ErrorState, FormError, Skeleton, TextField } from '@/shared/ui/ui'
import { useAuth } from '@/shared/state/auth'

const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/

function SetupForm({ tokenRequired }: { tokenRequired: boolean }) {
  const { signIn } = useAuth()
  const [form, setForm] = useState({ name: '', email: '', password: '', confirm: '', setupToken: '' })
  const [errors, setErrors] = useState<Partial<Record<keyof typeof form, string>>>({})
  const mutation = useSetup(signIn)
  const set = (key: keyof typeof form) => (value: string) => setForm((f) => ({ ...f, [key]: value }))

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const next: typeof errors = {}
    if (!form.name.trim()) next.name = 'Indiquez votre prénom ou votre nom.'
    if (!EMAIL.test(form.email.trim())) next.email = 'Saisissez une adresse e-mail valide.'
    if (form.password.length < 10) next.password = 'Le mot de passe doit contenir au moins 10 caractères.'
    if (form.confirm !== form.password) next.confirm = 'Les deux mots de passe ne correspondent pas.'
    if (tokenRequired && !form.setupToken.trim()) next.setupToken = 'Saisissez le code de configuration.'
    setErrors(next)
    if (Object.keys(next).length > 0) return
    mutation.mutate({
      name: form.name.trim(),
      email: form.email.trim(),
      password: form.password,
      // Envoyé seulement quand le serveur l'exige.
      ...(tokenRequired ? { setup_token: form.setupToken.trim() } : {}),
    })
  }

  return (
    <form onSubmit={submit} noValidate className="flex flex-col gap-4">
      <div>
        <h1 className="text-[26px] font-semibold">Première configuration</h1>
        <p className="mt-1.5 text-muted">
          Créez le compte unique de votre foyer. Vous serez aussi le premier membre ; vous pourrez en ajouter d’autres
          dans les paramètres.
        </p>
      </div>
      <TextField label="Votre nom" value={form.name} onChange={set('name')} error={errors.name} autoComplete="name" />
      <TextField
        label="Adresse e-mail"
        type="email"
        value={form.email}
        onChange={set('email')}
        error={errors.email}
        autoComplete="email"
      />
      <TextField
        label="Mot de passe"
        type="password"
        value={form.password}
        onChange={set('password')}
        error={errors.password}
        hint="10 caractères minimum."
        autoComplete="new-password"
      />
      <TextField
        label="Confirmez le mot de passe"
        type="password"
        value={form.confirm}
        onChange={set('confirm')}
        error={errors.confirm}
        autoComplete="new-password"
      />
      {tokenRequired ? (
        <TextField
          label="Code de configuration"
          type="password"
          value={form.setupToken}
          onChange={set('setupToken')}
          error={errors.setupToken}
          hint="La valeur de SETUP_TOKEN définie sur le serveur, visible dans le tableau de bord de l’hébergeur."
          autoComplete="off"
        />
      ) : null}
      <FormError error={mutation.error} />
      <Button type="submit" variant="primary" busy={mutation.isPending}>
        Créer mon compte
      </Button>
    </form>
  )
}

function LoginForm() {
  const { signIn } = useAuth()
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [errors, setErrors] = useState<{ email?: string; password?: string }>({})
  const mutation = useLogin(signIn)

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const next: typeof errors = {}
    if (!EMAIL.test(email.trim())) next.email = 'Saisissez une adresse e-mail valide.'
    if (!password) next.password = 'Saisissez votre mot de passe.'
    setErrors(next)
    if (Object.keys(next).length > 0) return
    mutation.mutate({ email: email.trim(), password })
  }

  return (
    <form onSubmit={submit} noValidate className="flex flex-col gap-4">
      <h1 className="text-[26px] font-semibold">Connexion</h1>
      <TextField
        label="Adresse e-mail"
        type="email"
        value={email}
        onChange={setEmail}
        error={errors.email}
        autoComplete="email"
      />
      <TextField
        label="Mot de passe"
        type="password"
        value={password}
        onChange={setPassword}
        error={errors.password}
        autoComplete="current-password"
      />
      <FormError error={mutation.error} />
      <Button type="submit" variant="primary" busy={mutation.isPending}>
        Se connecter
      </Button>
    </form>
  )
}

/** Motif de la marque : des strates de largeurs inégales, comme les couches d'un patrimoine. */
function Strata() {
  const bars = [
    ['var(--c1)', '86%'],
    ['var(--c2)', '58%'],
    ['var(--c3)', '72%'],
    ['var(--c5)', '34%'],
    ['var(--c4)', '47%'],
  ]
  return (
    <div className="flex flex-col gap-2.5" aria-hidden="true">
      {bars.map(([color, width]) => (
        <span key={color} className="h-3.5 rounded-full" style={{ background: color, width }} />
      ))}
    </div>
  )
}

export default function AuthPage() {
  const { isAuthenticated } = useAuth()
  const location = useLocation()
  const status = useAuthStatus()

  if (isAuthenticated) {
    const from = (location.state as { from?: string } | null)?.from
    return <Navigate to={from && from !== '/connexion' ? from : '/'} replace />
  }

  return (
    <div className="mx-auto grid min-h-dvh w-full max-w-[1040px] items-center gap-10 px-5 py-10 md:grid-cols-[1fr_minmax(0,420px)] md:gap-16">
      <div className="flex flex-col gap-7">
        <div className="flex items-center gap-3">
          <Logo size={36} />
          <span className="font-display text-[26px] font-semibold tracking-tight">FineMe</span>
        </div>
        <p className="max-w-[18ch] font-display text-[34px] leading-[1.08] font-semibold tracking-tight md:text-[46px]">
          Tout le patrimoine du foyer, strate par strate.
        </p>
        <div className="hidden max-w-[360px] md:block">
          <Strata />
        </div>
      </div>

      <div className="panel p-5 sm:p-7">
        {status.isPending ? (
          <div role="status" aria-label="Chargement…" className="flex flex-col gap-3">
            <Skeleton className="h-8 w-2/3" />
            <Skeleton className="h-11" />
            <Skeleton className="h-11" />
          </div>
        ) : status.isError ? (
          <ErrorState error={status.error} onRetry={() => void status.refetch()} />
        ) : status.data.setup_required ? (
          <SetupForm tokenRequired={status.data.setup_token_required === true} />
        ) : (
          <LoginForm />
        )}
      </div>
    </div>
  )
}
