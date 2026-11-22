import { useState, type FormEvent } from 'react'
import type {
  Account,
  AccountDetail,
  AccountInput,
  AccountKind,
  BuildingType,
  Member,
  PropertyCategory,
} from '@/shared/api/types'
import { parseFrNumber } from '@/shared/lib/parse'
import { duration, number, toInput, todayIso } from '@/shared/lib/format'
import {
  ACCOUNT_KIND_LABEL,
  BUILDING_TYPE_LABEL,
  ENVELOPES,
  PROPERTY_CATEGORY_LABEL,
} from '@/shared/lib/labels'
import {
  Button,
  ErrorState,
  FormError,
  IconButton,
  LoadingBlock,
  Modal,
  NumberField,
  SelectField,
  TextField,
} from '@/shared/ui/ui'

type OwnerRow = { member_id: string; share: string }

type FormState = {
  name: string
  institution: string
  kind: AccountKind
  envelope: string
  currency: string
  cash_balance: string
  annual_yield_pct: string
  management_fee_pct: string
  opened_at: string
  owners: OwnerRow[]
  // bien immobilier
  address: string
  category: PropertyCategory
  building_type: BuildingType
  surface_m2: string
  purchase_price: string
  purchase_fees: string
  purchased_at: string
  current_value: string
  monthly_rent: string
  monthly_charges: string
  yearly_property_tax: string
  // prêt
  property_account_id: string
  principal: string
  interest_rate_pct: string
  insurance_rate_pct: string
  duration_months: string
  start_date: string
}

type Errors = Partial<Record<keyof FormState, string>>

function initialState(
  detail: AccountDetail | undefined,
  kind: AccountKind,
  members: Member[],
  propertyAccountId: string | undefined,
): FormState {
  const primary = members.find((member) => member.is_primary) ?? members[0]
  const p = detail?.property
  const l = detail?.loan
  return {
    name: detail?.name ?? '',
    institution: detail?.institution ?? '',
    kind: detail?.kind ?? kind,
    envelope: detail?.envelope ?? '',
    currency: detail?.currency ?? 'EUR',
    cash_balance: detail ? toInput(detail.cash_balance, 2) : '',
    annual_yield_pct: toInput(detail?.annual_yield_pct),
    management_fee_pct: toInput(detail?.management_fee_pct),
    opened_at: detail?.opened_at ?? '',
    owners: detail
      ? detail.owners.map((owner) => ({ member_id: owner.member_id, share: toInput(owner.share * 100, 4) }))
      : primary
        ? [{ member_id: primary.id, share: '100' }]
        : [],
    address: p?.address ?? '',
    category: p?.category ?? 'primary',
    building_type: p?.building_type ?? 'apartment',
    surface_m2: toInput(p?.surface_m2),
    purchase_price: toInput(p?.purchase_price, 2),
    purchase_fees: toInput(p?.purchase_fees, 2),
    purchased_at: p?.purchased_at ?? '',
    current_value: toInput(p?.current_value, 2),
    monthly_rent: p && p.monthly_rent > 0 ? toInput(p.monthly_rent, 2) : '',
    monthly_charges: p && p.monthly_charges > 0 ? toInput(p.monthly_charges, 2) : '',
    yearly_property_tax: p && p.yearly_property_tax > 0 ? toInput(p.yearly_property_tax, 2) : '',
    property_account_id: l?.property_account_id ?? propertyAccountId ?? '',
    principal: toInput(l?.principal, 2),
    interest_rate_pct: toInput(l?.interest_rate_pct),
    insurance_rate_pct: toInput(l?.insurance_rate_pct),
    duration_months: toInput(l?.duration_months, 0),
    start_date: l?.start_date ?? todayIso(),
  }
}

/** Champ numérique facultatif : vide → `fallback`, illisible → erreur. */
function readNumber(
  form: FormState,
  errors: Errors,
  key: keyof FormState,
  options: { required?: boolean; min?: number; positive?: boolean } = {},
): number | null {
  const raw = String(form[key]).trim()
  if (!raw) {
    if (options.required) errors[key] = 'Ce champ est obligatoire.'
    return null
  }
  const value = parseFrNumber(raw)
  if (value === null) {
    errors[key] = 'Saisissez un nombre, par exemple 1 234,56.'
    return null
  }
  if (options.positive && value <= 0) {
    errors[key] = 'La valeur doit être supérieure à 0.'
    return null
  }
  if (options.min !== undefined && value < options.min) {
    errors[key] = `La valeur doit être au moins ${number(options.min)}.`
    return null
  }
  return value
}

export type AccountFormModalProps = {
  /** Compte à modifier ; absent = création. */
  accountId?: string
  /** Types proposés à la création. Un seul type = sélecteur masqué. */
  kinds: AccountKind[]
  /** Bien présélectionné pour un nouveau prêt. */
  propertyAccountId?: string
  onClose: () => void
  // Données et actions fournies par le hook `useAccountEditor` de la feature appelante.
  loading: boolean
  loadError: unknown
  members: Member[]
  detail: AccountDetail | undefined
  /** Tous les comptes : sert à proposer les biens finançables par un prêt. */
  accounts: Account[]
  onSubmit: (input: AccountInput) => void
  saving: boolean
  saveError: unknown
}

function titleFor(accountId: string | undefined, kinds: AccountKind[]) {
  if (accountId) return 'Modifier le compte'
  if (kinds.length === 1 && kinds[0] === 'real_estate') return 'Ajouter un bien'
  if (kinds.length === 1 && kinds[0] === 'loan') return 'Ajouter un prêt'
  return 'Ajouter un compte'
}

/** Formulaire de compte, de bien ou de prêt. Présentation seule : aucune requête ici. */
export function AccountFormModal(props: AccountFormModalProps) {
  const title = titleFor(props.accountId, props.kinds)
  if (props.loading) {
    return (
      <Modal title={title} onClose={props.onClose}>
        <LoadingBlock />
      </Modal>
    )
  }
  if (props.loadError) {
    return (
      <Modal title={title} onClose={props.onClose}>
        <ErrorState error={props.loadError} />
      </Modal>
    )
  }
  return <AccountFormInner {...props} title={title} />
}

function AccountFormInner({
  accountId,
  kinds,
  propertyAccountId,
  onClose,
  title,
  members,
  detail,
  accounts,
  onSubmit,
  saving,
  saveError,
}: AccountFormModalProps & { title: string }) {
  const [form, setForm] = useState<FormState>(() => initialState(detail, kinds[0], members, propertyAccountId))
  const [errors, setErrors] = useState<Errors>({})
  const set =
    <K extends keyof FormState>(key: K) =>
    (value: FormState[K]) =>
      setForm((f) => ({ ...f, [key]: value }))
  const text = (key: keyof FormState) => (value: string) => setForm((f) => ({ ...f, [key]: value }))

  const kind = form.kind
  const isProperty = kind === 'real_estate'
  const isLoan = kind === 'loan'
  const isPlain = !isProperty && !isLoan
  const totalShare = form.owners.reduce((sum, owner) => sum + (parseFrNumber(owner.share) ?? 0), 0)
  const properties = accounts.filter((account) => account.kind === 'real_estate' && account.id !== accountId)
  const months = parseFrNumber(form.duration_months)

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const next: Errors = {}
    if (!form.name.trim()) next.name = 'Donnez un nom à ce compte.'

    // Propriétaires
    const owners: AccountInput['owners'] = []
    const seen = new Set<string>()
    if (form.owners.length === 0) next.owners = 'Ajoutez au moins un propriétaire.'
    for (const owner of form.owners) {
      const share = parseFrNumber(owner.share)
      if (!owner.member_id) next.owners = 'Choisissez un membre pour chaque ligne.'
      else if (seen.has(owner.member_id)) next.owners = 'Un même membre ne peut apparaître qu’une fois.'
      else if (share === null || share <= 0 || share > 100) next.owners = 'Chaque part doit être comprise entre 0 et 100 %.'
      else owners.push({ member_id: owner.member_id, share: share / 100 })
      seen.add(owner.member_id)
    }
    if (!next.owners && totalShare > 100.0001) next.owners = 'Le total des parts ne peut pas dépasser 100 %.'

    const input: AccountInput = { name: form.name.trim(), kind, owners }

    if (isPlain) {
      const cash = readNumber(form, next, 'cash_balance')
      const yieldPct = readNumber(form, next, 'annual_yield_pct', { min: 0 })
      const feePct = readNumber(form, next, 'management_fee_pct', { min: 0 })
      if (feePct !== null && feePct > 100) next.management_fee_pct = 'Saisissez un pourcentage entre 0 et 100.'
      const currency = form.currency.trim().toUpperCase()
      if (!/^[A-Z]{3}$/.test(currency)) next.currency = 'Code devise sur 3 lettres, par exemple EUR.'
      input.institution = form.institution.trim() || null
      input.envelope = form.envelope || null
      input.currency = currency
      input.cash_balance = cash ?? 0
      input.annual_yield_pct = yieldPct
      if (kind === 'investment') input.management_fee_pct = feePct
      input.opened_at = form.opened_at || null
    }

    if (isProperty) {
      const purchasePrice = readNumber(form, next, 'purchase_price', { required: true, min: 0 })
      const currentValue = readNumber(form, next, 'current_value', { required: true, min: 0 })
      const fees = readNumber(form, next, 'purchase_fees', { min: 0 })
      const surface = readNumber(form, next, 'surface_m2', { positive: true })
      const rent = readNumber(form, next, 'monthly_rent', { min: 0 })
      const charges = readNumber(form, next, 'monthly_charges', { min: 0 })
      const tax = readNumber(form, next, 'yearly_property_tax', { min: 0 })
      input.currency = 'EUR'
      input.property = {
        address: form.address.trim() || null,
        category: form.category,
        building_type: form.building_type,
        surface_m2: surface,
        purchase_price: purchasePrice ?? 0,
        purchase_fees: fees ?? 0,
        purchased_at: form.purchased_at || null,
        current_value: currentValue ?? 0,
        monthly_rent: rent ?? 0,
        monthly_charges: charges ?? 0,
        yearly_property_tax: tax ?? 0,
      }
    }

    if (isLoan) {
      const principal = readNumber(form, next, 'principal', { required: true, positive: true })
      const rate = readNumber(form, next, 'interest_rate_pct', { required: true, min: 0 })
      const insurance = readNumber(form, next, 'insurance_rate_pct', { min: 0 })
      const durationMonths = readNumber(form, next, 'duration_months', { required: true, positive: true })
      if (durationMonths !== null && !Number.isInteger(durationMonths))
        next.duration_months = 'Saisissez un nombre entier de mois.'
      if (!form.start_date) next.start_date = 'Indiquez la date de début du prêt.'
      input.institution = form.institution.trim() || null
      input.currency = 'EUR'
      input.loan = {
        property_account_id: form.property_account_id || null,
        principal: principal ?? 0,
        interest_rate_pct: rate ?? 0,
        insurance_rate_pct: insurance ?? 0,
        duration_months: durationMonths ?? 0,
        start_date: form.start_date,
      }
    }

    setErrors(next)
    if (Object.keys(next).length > 0) return
    onSubmit(input)
  }

  const memberOptions = [
    { value: '', label: 'Choisir…' },
    ...members.map((member) => ({ value: member.id, label: member.name })),
  ]

  return (
    <Modal
      title={title}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Annuler</Button>
          <Button variant="primary" type="submit" form="account-form" busy={saving}>
            {accountId ? 'Enregistrer' : 'Ajouter'}
          </Button>
        </>
      }
    >
      <form id="account-form" onSubmit={submit} noValidate className="flex flex-col gap-4">
        {!accountId && kinds.length > 1 ? (
          <SelectField
            label="Type de compte"
            value={kind}
            onChange={(value) => set('kind')(value as AccountKind)}
            options={kinds.map((k) => ({ value: k, label: ACCOUNT_KIND_LABEL[k] }))}
          />
        ) : null}

        <TextField
          label={isProperty ? 'Nom du bien' : isLoan ? 'Nom du prêt' : 'Nom du compte'}
          value={form.name}
          onChange={text('name')}
          error={errors.name}
        />

        {!isProperty ? (
          <TextField
            label={isLoan ? 'Banque prêteuse' : 'Établissement'}
            value={form.institution}
            onChange={text('institution')}
            hint="Facultatif."
          />
        ) : null}

        {isPlain ? (
          <>
            <div className="grid gap-4 sm:grid-cols-2">
              <SelectField
                label="Enveloppe fiscale"
                value={form.envelope}
                onChange={text('envelope')}
                options={[
                  { value: '', label: 'Aucune' },
                  ...ENVELOPES,
                  ...(form.envelope && !ENVELOPES.some((e) => e.value === form.envelope)
                    ? [{ value: form.envelope, label: form.envelope }]
                    : []),
                ]}
              />
              <TextField
                label="Devise"
                value={form.currency}
                onChange={text('currency')}
                error={errors.currency}
                maxLength={3}
                autoCapitalize="characters"
              />
            </div>
            <div className="grid gap-4 sm:grid-cols-2">
              <NumberField
                label="Solde espèces"
                value={form.cash_balance}
                onChange={text('cash_balance')}
                error={errors.cash_balance}
                hint={kind === 'investment' || kind === 'crypto' ? 'Hors positions, ajoutées ensuite.' : undefined}
              />
              <NumberField
                label="Rendement annuel"
                value={form.annual_yield_pct}
                onChange={text('annual_yield_pct')}
                error={errors.annual_yield_pct}
                suffix="%"
                hint="Facultatif, par exemple le taux d’un livret."
              />
              {kind === 'investment' ? (
                <NumberField
                  label="Frais de gestion de l’enveloppe"
                  value={form.management_fee_pct}
                  onChange={text('management_fee_pct')}
                  error={errors.management_fee_pct}
                  suffix="%"
                  hint="Facultatif : frais annuels d’une assurance-vie ou d’un PER, en % de l’encours."
                />
              ) : null}
            </div>
            <TextField label="Date d’ouverture" type="date" value={form.opened_at} onChange={text('opened_at')} hint="Facultatif." />
          </>
        ) : null}

        {isProperty ? (
          <>
            <TextField label="Adresse" value={form.address} onChange={text('address')} hint="Facultatif." />
            <div className="grid gap-4 sm:grid-cols-2">
              <SelectField
                label="Usage"
                value={form.category}
                onChange={(value) => set('category')(value as PropertyCategory)}
                options={Object.entries(PROPERTY_CATEGORY_LABEL).map(([value, label]) => ({ value, label }))}
              />
              <SelectField
                label="Type de bien"
                value={form.building_type}
                onChange={(value) => set('building_type')(value as BuildingType)}
                options={Object.entries(BUILDING_TYPE_LABEL).map(([value, label]) => ({ value, label }))}
              />
            </div>
            <div className="grid gap-4 sm:grid-cols-2">
              <NumberField
                label="Valeur actuelle estimée"
                value={form.current_value}
                onChange={text('current_value')}
                error={errors.current_value}
                suffix="€"
              />
              <NumberField
                label="Surface"
                value={form.surface_m2}
                onChange={text('surface_m2')}
                error={errors.surface_m2}
                suffix="m²"
              />
              <NumberField
                label="Prix d’achat"
                value={form.purchase_price}
                onChange={text('purchase_price')}
                error={errors.purchase_price}
                suffix="€"
              />
              <NumberField
                label="Frais d’acquisition"
                value={form.purchase_fees}
                onChange={text('purchase_fees')}
                error={errors.purchase_fees}
                suffix="€"
                hint="Notaire et agence."
              />
            </div>
            <TextField label="Date d’achat" type="date" value={form.purchased_at} onChange={text('purchased_at')} hint="Facultatif." />
            <div className="grid gap-4 sm:grid-cols-3">
              <NumberField
                label="Loyer mensuel"
                value={form.monthly_rent}
                onChange={text('monthly_rent')}
                error={errors.monthly_rent}
                suffix="€"
                hint="Vide si non loué."
              />
              <NumberField
                label="Charges mensuelles"
                value={form.monthly_charges}
                onChange={text('monthly_charges')}
                error={errors.monthly_charges}
                suffix="€"
                hint="Non récupérables."
              />
              <NumberField
                label="Taxe foncière annuelle"
                value={form.yearly_property_tax}
                onChange={text('yearly_property_tax')}
                error={errors.yearly_property_tax}
                suffix="€"
              />
            </div>
          </>
        ) : null}

        {isLoan ? (
          <>
            <SelectField
              label="Bien financé"
              value={form.property_account_id}
              onChange={text('property_account_id')}
              options={[
                { value: '', label: 'Aucun (prêt personnel, auto…)' },
                ...properties.map((account) => ({ value: account.id, label: account.name })),
              ]}
            />
            <div className="grid gap-4 sm:grid-cols-2">
              <NumberField
                label="Capital emprunté"
                value={form.principal}
                onChange={text('principal')}
                error={errors.principal}
                suffix="€"
              />
              <NumberField
                label="Durée"
                value={form.duration_months}
                onChange={text('duration_months')}
                error={errors.duration_months}
                suffix="mois"
                hint={months && months >= 12 && Number.isInteger(months) ? `Soit ${duration(months)}.` : undefined}
              />
              <NumberField
                label="Taux nominal annuel"
                value={form.interest_rate_pct}
                onChange={text('interest_rate_pct')}
                error={errors.interest_rate_pct}
                suffix="%"
              />
              <NumberField
                label="Taux d’assurance annuel"
                value={form.insurance_rate_pct}
                onChange={text('insurance_rate_pct')}
                error={errors.insurance_rate_pct}
                suffix="%"
                hint="Sur le capital initial. Vide si aucune."
              />
            </div>
            <TextField
              label="Date de début"
              type="date"
              value={form.start_date}
              onChange={text('start_date')}
              error={errors.start_date}
            />
          </>
        ) : null}

        <fieldset className="flex flex-col gap-2.5 rounded-[11px] border border-line p-3.5">
          <legend className="px-1 text-[13.5px] font-medium">Propriétaires et parts</legend>
          {members.length === 0 ? (
            <p className="text-muted">Aucun membre dans le foyer. Ajoutez-en un dans les paramètres.</p>
          ) : null}
          {form.owners.map((owner, index) => (
            <div key={index} className="flex items-end gap-2">
              <SelectField
                label="Membre"
                fieldClassName="flex-1"
                value={owner.member_id}
                onChange={(value) =>
                  set('owners')(form.owners.map((o, i) => (i === index ? { ...o, member_id: value } : o)))
                }
                options={memberOptions}
              />
              <NumberField
                label="Part"
                fieldClassName="w-24"
                value={owner.share}
                onChange={(value) => set('owners')(form.owners.map((o, i) => (i === index ? { ...o, share: value } : o)))}
                suffix="%"
              />
              <IconButton
                icon="trash"
                label="Retirer ce propriétaire"
                className="mb-0.5"
                onClick={() => set('owners')(form.owners.filter((_, i) => i !== index))}
              />
            </div>
          ))}
          <div className="flex flex-wrap items-center justify-between gap-2">
            <Button
              size="sm"
              icon="plus"
              disabled={form.owners.length >= members.length}
              onClick={() => {
                const free = members.find((member) => !form.owners.some((o) => o.member_id === member.id))
                const rest = Math.max(0, 100 - totalShare)
                set('owners')([...form.owners, { member_id: free?.id ?? '', share: rest > 0 ? toInput(rest, 4) : '' }])
              }}
            >
              Ajouter un propriétaire
            </Button>
            <span className={`num text-[13.5px] ${totalShare > 100.0001 ? 'text-neg' : 'text-muted'}`}>
              Total : {number(totalShare, 2)} %
            </span>
          </div>
          {errors.owners ? <p className="text-[13px] text-neg">{errors.owners}</p> : null}
          {!errors.owners && totalShare > 0 && totalShare < 99.9999 ? (
            <p className="text-[13px] text-muted">
              Le reste ({number(100 - totalShare, 2)} %) est considéré comme détenu hors du foyer.
            </p>
          ) : null}
        </fieldset>

        <FormError error={saveError} />
      </form>
    </Modal>
  )
}
