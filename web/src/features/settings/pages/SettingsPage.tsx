import { useRef, useState, type ChangeEvent, type FormEvent } from 'react'
import { Link } from 'react-router'
import { useMembers } from '@/shared/api/queries'
import {
  useChangePassword,
  useDeleteMember,
  useExportBundle,
  useImportBundle,
  useSaveMember,
} from '../hooks/useSettingsApi'
import { ExportPanel } from '../components/ExportPanel'
import { LoginHistoryPanel } from '../components/LoginHistoryPanel'
import { downloadFile } from '../lib/download'
import type { Bundle, ImportResult, Member } from '@/shared/api/types'
import {
  Badge,
  Button,
  ConfirmModal,
  Dot,
  ErrorState,
  FormError,
  IconButton,
  LoadingBlock,
  Modal,
  Notice,
  PageHeader,
  Panel,
  Segmented,
  TextField,
} from '@/shared/ui/ui'
import { todayIso } from '@/shared/lib/format'
import { MEMBER_COLORS } from '@/shared/lib/labels'
import { MIN_PASSWORD_LENGTH, newPasswordErrors } from '@/shared/lib/password'
import { useAuth } from '@/shared/state/auth'
import { useTheme, type ThemeChoice } from '@/shared/state/theme'

function MemberFormModal({ member, usedColors, onClose }: { member?: Member; usedColors: string[]; onClose: () => void }) {
  const [name, setName] = useState(member?.name ?? '')
  const [color, setColor] = useState(
    member?.color ?? MEMBER_COLORS.find((c) => !usedColors.includes(c)) ?? MEMBER_COLORS[0],
  )
  const [nameError, setNameError] = useState('')
  const mutation = useSaveMember(member?.id, onClose)
  const palette = MEMBER_COLORS.some((c) => c.toLowerCase() === color.toLowerCase()) ? MEMBER_COLORS : [color, ...MEMBER_COLORS]

  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (!name.trim()) {
      setNameError('Indiquez le prénom ou le nom du membre.')
      return
    }
    setNameError('')
    mutation.mutate({ name: name.trim(), color })
  }

  return (
    <Modal
      title={member ? 'Modifier le membre' : 'Ajouter un membre'}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Annuler</Button>
          <Button variant="primary" type="submit" form="member-form" busy={mutation.isPending}>
            {member ? 'Enregistrer' : 'Ajouter'}
          </Button>
        </>
      }
    >
      <form id="member-form" onSubmit={submit} noValidate className="flex flex-col gap-4">
        <TextField label="Nom" value={name} onChange={setName} error={nameError} />
        <fieldset>
          <legend className="mb-2 text-[13.5px] font-medium">Couleur</legend>
          <div className="flex flex-wrap gap-2.5">
            {palette.map((option) => (
              <label key={option} className="relative">
                <input
                  type="radio"
                  name="member-color"
                  value={option}
                  checked={option.toLowerCase() === color.toLowerCase()}
                  onChange={() => setColor(option)}
                  className="peer sr-only"
                />
                <span
                  className="block size-9 cursor-pointer rounded-full ring-offset-2 ring-offset-surface peer-checked:ring-2 peer-checked:ring-ink peer-focus-visible:ring-2 peer-focus-visible:ring-accent"
                  style={{ background: option }}
                />
                <span className="sr-only">Couleur {option}</span>
              </label>
            ))}
          </div>
        </fieldset>
        <FormError error={mutation.error} />
      </form>
    </Modal>
  )
}

function MembersPanel() {
  const members = useMembers()
  const [creating, setCreating] = useState(false)
  const [editing, setEditing] = useState<Member | null>(null)
  const [deleting, setDeleting] = useState<Member | null>(null)
  const remove = useDeleteMember(() => setDeleting(null))

  return (
    <Panel
      title="Membres du foyer"
      flush
      action={
        <Button size="sm" variant="primary" icon="plus" onClick={() => setCreating(true)}>
          Ajouter un membre
        </Button>
      }
    >
      {members.isPending ? (
        <div className="px-4 pb-4 sm:px-5">
          <LoadingBlock rows={1} />
        </div>
      ) : members.isError ? (
        <div className="px-4 pb-4 sm:px-5">
          <ErrorState error={members.error} onRetry={() => void members.refetch()} />
        </div>
      ) : members.data.length === 0 ? (
        <p className="px-4 pb-5 text-muted sm:px-5">
          Aucun membre. Ajoutez-en un pour pouvoir lui attribuer des comptes.
        </p>
      ) : (
        <ul className="mt-1">
          {members.data.map((member) => (
            <li key={member.id} className="flex items-center gap-3 border-b border-line py-2 pr-2 pl-4 last:border-0 sm:pl-5">
              <Dot color={member.color} />
              <span className="min-w-0 flex-1 truncate font-medium">{member.name}</span>
              {member.is_primary ? <Badge tone="accent">Principal</Badge> : null}
              <IconButton icon="edit" label={`Modifier ${member.name}`} onClick={() => setEditing(member)} />
              <IconButton
                icon="trash"
                label={`Supprimer ${member.name}`}
                disabled={member.is_primary}
                onClick={() => {
                  remove.reset()
                  setDeleting(member)
                }}
              />
            </li>
          ))}
        </ul>
      )}

      {creating || editing ? (
        <MemberFormModal
          member={editing ?? undefined}
          usedColors={(members.data ?? []).map((member) => member.color)}
          onClose={() => {
            setCreating(false)
            setEditing(null)
          }}
        />
      ) : null}
      {deleting ? (
        <ConfirmModal
          title="Supprimer ce membre ?"
          confirmLabel="Supprimer le membre"
          busy={remove.isPending}
          error={remove.error}
          onConfirm={() => remove.mutate(deleting.id)}
          onClose={() => setDeleting(null)}
        >
          « {deleting.name} » sera retiré du foyer. C’est impossible tant qu’il possède une part d’un compte.
        </ConfirmModal>
      ) : null}
    </Panel>
  )
}

function isBundle(value: unknown): value is Bundle {
  if (!value || typeof value !== 'object') return false
  const bundle = value as Record<string, unknown>
  return (
    bundle.version === 1 && Array.isArray(bundle.members) && Array.isArray(bundle.accounts) && Array.isArray(bundle.goals)
  )
}

function DataPanel() {
  const fileInput = useRef<HTMLInputElement>(null)
  const [pending, setPending] = useState<{ name: string; bundle: Bundle } | null>(null)
  const [fileError, setFileError] = useState('')
  const [imported, setImported] = useState<ImportResult | null>(null)

  const exportData = useExportBundle((bundle) =>
    downloadFile(JSON.stringify(bundle, null, 2), `fineme-export-${todayIso()}.json`, 'application/json'),
  )
  const importData = useImportBundle((result) => {
    setImported(result)
    setPending(null)
  })

  const onFile = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]
    event.target.value = ''
    setFileError('')
    setImported(null)
    if (!file) return
    try {
      const parsed: unknown = JSON.parse(await file.text())
      if (!isBundle(parsed)) {
        setFileError('Ce fichier n’est pas un export FineMe : il doit contenir « version: 1 », des membres, des comptes et des objectifs.')
        return
      }
      importData.reset()
      setPending({ name: file.name, bundle: parsed })
    } catch {
      setFileError('Lecture impossible : le fichier n’est pas un JSON valide.')
    }
  }

  return (
    <Panel title="Vos données">
      <div className="flex flex-col gap-5">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <p className="max-w-[56ch] text-muted">
            L’export regroupe membres, comptes, positions, biens, prêts et objectifs dans un fichier JSON à conserver en
            lieu sûr.
          </p>
          <Button icon="download" busy={exportData.isPending} onClick={() => exportData.mutate()}>
            Exporter mes données
          </Button>
        </div>
        <FormError error={exportData.error} />

        <div className="flex flex-wrap items-center justify-between gap-3 border-t border-line pt-5">
          <p className="max-w-[56ch] text-muted">
            L’import ajoute ou met à jour les éléments du fichier sans rien supprimer. Le relancer avec le même fichier
            ne crée pas de doublons.
          </p>
          <Button icon="upload" onClick={() => fileInput.current?.click()}>
            Importer un fichier
          </Button>
          <input
            ref={fileInput}
            type="file"
            accept="application/json,.json"
            className="sr-only"
            tabIndex={-1}
            aria-label="Fichier d’export FineMe à importer"
            onChange={(event) => void onFile(event)}
          />
        </div>
        {fileError ? <FormError error={fileError} /> : null}
        {imported ? (
          <Notice tone="pos">
            Import terminé : {imported.members} membre(s), {imported.accounts} compte(s), {imported.holdings} position(s)
            et {imported.goals} objectif(s) traités.
          </Notice>
        ) : null}
      </div>

      {pending ? (
        <Modal
          title="Importer ce fichier ?"
          onClose={() => setPending(null)}
          footer={
            <>
              <Button onClick={() => setPending(null)}>Annuler</Button>
              <Button variant="primary" icon="upload" busy={importData.isPending} onClick={() => importData.mutate(pending.bundle)}>
                Importer
              </Button>
            </>
          }
        >
          <div className="flex flex-col gap-3">
            <p>
              <span className="font-medium">{pending.name}</span> contient {pending.bundle.members.length} membre(s),{' '}
              {pending.bundle.accounts.length} compte(s) et {pending.bundle.goals.length} objectif(s).
            </p>
            <p className="text-muted">Les éléments déjà présents seront mis à jour avec les valeurs du fichier.</p>
            <FormError error={importData.error} />
          </div>
        </Modal>
      ) : null}
    </Panel>
  )
}

const EMPTY_PASSWORD_FORM = { current: '', password: '', confirm: '' }

function PasswordPanel() {
  const { renewSession } = useAuth()
  const [form, setForm] = useState(EMPTY_PASSWORD_FORM)
  const [errors, setErrors] = useState<Partial<Record<keyof typeof form, string>>>({})
  const [changed, setChanged] = useState(false)
  const mutation = useChangePassword((result) => {
    // Le serveur a fermé toutes les sessions : ce nouveau jeton garde cet appareil connecté.
    renewSession(result)
    setForm(EMPTY_PASSWORD_FORM)
    setChanged(true)
  })
  const set = (key: keyof typeof form) => (value: string) => {
    setChanged(false)
    setForm((f) => ({ ...f, [key]: value }))
  }

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const next: typeof errors = newPasswordErrors(form.password, form.confirm)
    if (!form.current) next.current = 'Saisissez votre mot de passe actuel.'
    else if (!next.password && form.password === form.current) {
      next.password = 'Le nouveau mot de passe doit être différent de l’actuel.'
    }
    setErrors(next)
    if (Object.keys(next).length > 0) return
    mutation.mutate({ current_password: form.current, new_password: form.password })
  }

  return (
    <Panel title="Mot de passe">
      <form onSubmit={submit} noValidate className="flex max-w-[420px] flex-col gap-4">
        <p className="text-muted">Après le changement, les autres appareils devront se reconnecter.</p>
        <TextField
          label="Mot de passe actuel"
          type="password"
          value={form.current}
          onChange={set('current')}
          error={errors.current}
          autoComplete="current-password"
        />
        <TextField
          label="Nouveau mot de passe"
          type="password"
          value={form.password}
          onChange={set('password')}
          error={errors.password}
          hint={`${MIN_PASSWORD_LENGTH} caractères minimum.`}
          autoComplete="new-password"
        />
        <TextField
          label="Confirmez le nouveau mot de passe"
          type="password"
          value={form.confirm}
          onChange={set('confirm')}
          error={errors.confirm}
          autoComplete="new-password"
        />
        <FormError error={mutation.error} />
        {changed ? (
          <Notice tone="pos">Mot de passe modifié. Les autres appareils ont été déconnectés.</Notice>
        ) : null}
        <div>
          <Button type="submit" variant="primary" busy={mutation.isPending}>
            Changer le mot de passe
          </Button>
        </div>
      </form>
    </Panel>
  )
}

export default function SettingsPage() {
  const { user, signOut } = useAuth()
  const { theme, setTheme } = useTheme()

  return (
    <>
      <PageHeader title="Paramètres" />
      <div className="flex flex-col gap-4 sm:gap-5">
        <MembersPanel />
        <ExportPanel />
        <DataPanel />
        <Panel title="Historique des graphiques">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <p className="max-w-[56ch] text-muted">
              Repérez les points suspects, corrigez ou supprimez des jours, recalculez le point du jour ou repartez de
              zéro.
            </p>
            <Link
              to="/parametres/historique"
              className="inline-flex min-h-10 items-center rounded-[9px] border border-line px-4 font-medium hover:border-muted"
            >
              Nettoyer l’historique
            </Link>
          </div>
        </Panel>
        <Panel title="Apparence">
          <Segmented<ThemeChoice>
            label="Thème"
            value={theme}
            onChange={setTheme}
            options={[
              { value: 'system', label: 'Comme l’appareil' },
              { value: 'light', label: 'Clair' },
              { value: 'dark', label: 'Sombre' },
            ]}
          />
        </Panel>
        <PasswordPanel />
        <LoginHistoryPanel />
        <Panel title="Session">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <p className="text-muted">
              {user ? (
                <>
                  Connecté en tant que <span className="text-ink">{user.name}</span> ({user.email}).
                </>
              ) : (
                'Vous êtes connecté.'
              )}
            </p>
            <Button icon="logout" onClick={signOut}>
              Se déconnecter
            </Button>
          </div>
        </Panel>
      </div>
    </>
  )
}
