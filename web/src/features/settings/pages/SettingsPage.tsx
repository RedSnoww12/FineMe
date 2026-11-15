import { useRef, useState, type ChangeEvent, type FormEvent } from 'react'
import { useMembers } from '@/shared/api/queries'
import { useDeleteMember, useExportBundle, useImportBundle, useSaveMember } from '../hooks/useSettingsApi'
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

  const exportData = useExportBundle((bundle) => {
      const url = URL.createObjectURL(new Blob([JSON.stringify(bundle, null, 2)], { type: 'application/json' }))
      const link = document.createElement('a')
      link.href = url
      link.download = `fineme-export-${todayIso()}.json`
      document.body.appendChild(link)
      link.click()
      link.remove()
      URL.revokeObjectURL(url)
  })
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

export default function SettingsPage() {
  const { user, signOut } = useAuth()
  const { theme, setTheme } = useTheme()

  return (
    <>
      <PageHeader title="Paramètres" />
      <div className="flex flex-col gap-4 sm:gap-5">
        <MembersPanel />
        <DataPanel />
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
