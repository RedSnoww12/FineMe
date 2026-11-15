import {
  useEffect,
  useId,
  useRef,
  type ButtonHTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
} from 'react'
import { ApiError, errorMessage } from '@/shared/api/client'
import { Icon, type IconName } from '@/shared/ui/Icon'

export function cx(...parts: (string | false | null | undefined)[]) {
  return parts.filter(Boolean).join(' ')
}

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: 'primary' | 'secondary' | 'ghost' | 'danger'
  size?: 'sm' | 'md'
  icon?: IconName
  busy?: boolean
}

const BUTTON_VARIANT = {
  primary: 'bg-accent text-accent-ink hover:opacity-90',
  secondary: 'bg-surface text-ink border border-line hover:border-muted',
  ghost: 'text-ink hover:bg-surface-2',
  danger: 'bg-neg-soft text-neg hover:opacity-85',
}

export function Button({
  variant = 'secondary',
  size = 'md',
  icon,
  busy = false,
  className,
  children,
  disabled,
  type = 'button',
  ...rest
}: ButtonProps) {
  return (
    <button
      type={type}
      disabled={disabled || busy}
      aria-busy={busy || undefined}
      className={cx(
        'inline-flex shrink-0 items-center justify-center gap-2 rounded-[9px] font-medium transition disabled:opacity-55',
        size === 'sm' ? 'min-h-9 px-3 text-[13.5px]' : 'min-h-[42px] px-4 text-[14.5px] sm:min-h-10',
        BUTTON_VARIANT[variant],
        className,
      )}
      {...rest}
    >
      {busy ? <Spinner /> : icon ? <Icon name={icon} size={size === 'sm' ? 16 : 17} /> : null}
      {children}
    </button>
  )
}

export function IconButton({
  icon,
  label,
  className,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { icon: IconName; label: string }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      className={cx(
        'inline-flex size-9 shrink-0 items-center justify-center rounded-[9px] text-muted transition hover:bg-surface-2 hover:text-ink disabled:opacity-50',
        className,
      )}
      {...rest}
    >
      <Icon name={icon} size={17} />
    </button>
  )
}

export function Spinner() {
  return (
    <span
      className="inline-block size-4 animate-spin rounded-full border-2 border-current border-r-transparent"
      aria-hidden="true"
    />
  )
}

type FieldProps = {
  label: string
  error?: string
  hint?: string
  children: (props: { id: string; 'aria-invalid': boolean; 'aria-describedby'?: string }) => ReactNode
  className?: string
}

export function Field({ label, error, hint, children, className }: FieldProps) {
  const id = useId()
  const describedBy = error ? `${id}-error` : hint ? `${id}-hint` : undefined
  return (
    <div className={cx('flex min-w-0 flex-col gap-1.5', className)}>
      <label htmlFor={id} className="text-[13.5px] font-medium">
        {label}
      </label>
      {children({ id, 'aria-invalid': Boolean(error), 'aria-describedby': describedBy })}
      {error ? (
        <p id={`${id}-error`} className="text-[13px] text-neg">
          {error}
        </p>
      ) : hint ? (
        <p id={`${id}-hint`} className="text-[13px] text-muted">
          {hint}
        </p>
      ) : null}
    </div>
  )
}

type TextFieldProps = Omit<InputHTMLAttributes<HTMLInputElement>, 'onChange' | 'value'> & {
  label: string
  value: string
  onChange: (value: string) => void
  error?: string
  hint?: string
  suffix?: string
  fieldClassName?: string
}

export function TextField({
  label,
  value,
  onChange,
  error,
  hint,
  suffix,
  fieldClassName,
  className,
  ...rest
}: TextFieldProps) {
  return (
    <Field label={label} error={error} hint={hint} className={fieldClassName}>
      {(a11y) => (
        <div className="relative">
          <input
            {...a11y}
            {...rest}
            value={value}
            onChange={(event) => onChange(event.target.value)}
            className={cx('input', suffix && 'pr-10', className)}
          />
          {suffix ? (
            <span className="pointer-events-none absolute inset-y-0 right-3 flex items-center text-[13.5px] text-muted">
              {suffix}
            </span>
          ) : null}
        </div>
      )}
    </Field>
  )
}

/** Champ numérique en saisie libre : accepte `1 234,56`. La conversion se fait à la validation. */
export function NumberField(props: TextFieldProps) {
  return <TextField inputMode="decimal" autoComplete="off" {...props} className="num" />
}

type SelectFieldProps = Omit<SelectHTMLAttributes<HTMLSelectElement>, 'onChange' | 'value'> & {
  label: string
  value: string
  onChange: (value: string) => void
  options: { value: string; label: string }[]
  error?: string
  hint?: string
  fieldClassName?: string
}

export function SelectField({ label, value, onChange, options, error, hint, fieldClassName, ...rest }: SelectFieldProps) {
  return (
    <Field label={label} error={error} hint={hint} className={fieldClassName}>
      {(a11y) => (
        <select {...a11y} {...rest} value={value} onChange={(event) => onChange(event.target.value)} className="input">
          {options.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      )}
    </Field>
  )
}

export function Panel({
  title,
  action,
  children,
  className,
  flush = false,
}: {
  title?: ReactNode
  action?: ReactNode
  children: ReactNode
  className?: string
  /** Sans marge intérieure : pour les tableaux bord à bord. */
  flush?: boolean
}) {
  return (
    <section className={cx('panel min-w-0', className)}>
      {title || action ? (
        <header className="flex min-h-[52px] flex-wrap items-center justify-between gap-x-3 gap-y-2 px-4 pt-3 sm:px-5">
          {title ? <h2 className="text-[16px] font-semibold">{title}</h2> : <span />}
          {action}
        </header>
      ) : null}
      <div className={flush ? '' : cx('px-4 pb-4 sm:px-5 sm:pb-5', title || action ? 'pt-2' : 'pt-4 sm:pt-5')}>
        {children}
      </div>
    </section>
  )
}

export function PageHeader({
  title,
  description,
  actions,
  back,
}: {
  title: string
  description?: ReactNode
  actions?: ReactNode
  back?: ReactNode
}) {
  return (
    <header className="mb-5 flex flex-wrap items-end justify-between gap-x-4 gap-y-3 sm:mb-7">
      <div className="min-w-0">
        {back}
        <h1 className="text-[26px] font-semibold sm:text-[30px]">{title}</h1>
        {description ? <p className="mt-1.5 max-w-[68ch] text-muted">{description}</p> : null}
      </div>
      {actions ? <div className="flex flex-wrap items-center gap-2">{actions}</div> : null}
    </header>
  )
}

export function Stat({
  label,
  value,
  sub,
  tone,
}: {
  label: string
  value: ReactNode
  sub?: ReactNode
  tone?: 'pos' | 'neg'
}) {
  return (
    <div className="min-w-0">
      <div className="text-[13.5px] text-muted">{label}</div>
      <div
        className={cx(
          'num mt-0.5 truncate font-display text-[21px] font-semibold',
          tone === 'pos' && 'text-pos',
          tone === 'neg' && 'text-neg',
        )}
      >
        {value}
      </div>
      {sub ? <div className="num mt-0.5 text-[13px] text-muted">{sub}</div> : null}
    </div>
  )
}

export function Badge({
  children,
  tone = 'muted',
}: {
  children: ReactNode
  tone?: 'muted' | 'pos' | 'neg' | 'brass' | 'accent'
}) {
  const tones = {
    muted: 'bg-surface-2 text-muted',
    pos: 'bg-surface-2 text-pos',
    neg: 'bg-neg-soft text-neg',
    brass: 'bg-surface-2 text-brass',
    accent: 'bg-accent-soft text-accent',
  }
  return (
    <span className={cx('inline-flex items-center rounded-full px-2.5 py-0.5 text-[12.5px] font-medium', tones[tone])}>
      {children}
    </span>
  )
}

export function ProgressBar({ pct, tone = 'accent' }: { pct: number; tone?: 'accent' | 'pos' | 'brass' | 'neg' }) {
  const clamped = Math.max(0, Math.min(100, Number.isFinite(pct) ? pct : 0))
  const colors = { accent: 'bg-accent', pos: 'bg-pos', brass: 'bg-brass', neg: 'bg-neg' }
  return (
    <div
      className="h-2 overflow-hidden rounded-full bg-surface-2"
      role="progressbar"
      aria-valuenow={Math.round(clamped)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <div className={cx('h-full rounded-full', colors[tone])} style={{ width: `${clamped}%` }} />
    </div>
  )
}

export function Segmented<T extends string>({
  value,
  onChange,
  options,
  label,
}: {
  value: T
  onChange: (value: T) => void
  options: { value: T; label: string }[]
  label: string
}) {
  return (
    <div role="group" aria-label={label} className="inline-flex rounded-[10px] bg-surface-2 p-[3px]">
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          aria-pressed={option.value === value}
          onClick={() => onChange(option.value)}
          className={cx(
            'min-h-8 rounded-[8px] px-3 text-[13.5px] font-medium transition',
            option.value === value ? 'bg-surface text-ink shadow-[0_1px_2px_rgb(0_0_0/0.12)]' : 'text-muted hover:text-ink',
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  )
}

export function EmptyState({
  title,
  children,
  action,
}: {
  title: string
  children?: ReactNode
  action?: ReactNode
}) {
  return (
    <div className="flex flex-col items-start gap-3 rounded-[14px] border border-dashed border-line px-5 py-7 sm:px-7">
      <h2 className="text-[17px] font-semibold">{title}</h2>
      {children ? <div className="max-w-[62ch] text-muted">{children}</div> : null}
      {action ? <div className="mt-1 flex flex-wrap gap-2">{action}</div> : null}
    </div>
  )
}

export function ErrorState({ error, onRetry }: { error: unknown; onRetry?: () => void }) {
  const unreachable = error instanceof ApiError && error.unreachable
  return (
    <div role="alert" className="flex flex-col items-start gap-3 rounded-[14px] border border-neg/40 bg-neg-soft px-5 py-5">
      <div className="flex items-start gap-2.5 text-neg">
        <Icon name="alert" className="mt-0.5 shrink-0" />
        <div>
          <p className="font-semibold">{unreachable ? 'API injoignable' : 'Chargement impossible'}</p>
          <p className="mt-0.5 text-ink">{errorMessage(error).replace(/^API injoignable\.\s*/, '')}</p>
        </div>
      </div>
      {onRetry ? (
        <Button size="sm" icon="refresh" onClick={onRetry}>
          Réessayer
        </Button>
      ) : null}
    </div>
  )
}

/** Message d'erreur d'un formulaire ou d'une action. */
export function FormError({ error }: { error: unknown }) {
  if (!error) return null
  return (
    <p role="alert" className="flex items-start gap-2 rounded-[9px] bg-neg-soft px-3 py-2.5 text-[14px] text-neg">
      <Icon name="alert" size={17} className="mt-0.5 shrink-0" />
      <span>{typeof error === 'string' ? error : errorMessage(error)}</span>
    </p>
  )
}

export function Notice({ children, tone = 'muted' }: { children: ReactNode; tone?: 'muted' | 'pos' }) {
  return (
    <p
      role="status"
      className={cx(
        'rounded-[9px] px-3 py-2.5 text-[14px]',
        tone === 'pos' ? 'bg-surface-2 text-pos' : 'bg-surface-2 text-muted',
      )}
    >
      {children}
    </p>
  )
}

export function Skeleton({ className }: { className?: string }) {
  return <div className={cx('skeleton', className)} aria-hidden="true" />
}

export function LoadingBlock({ rows = 3, label = 'Chargement…' }: { rows?: number; label?: string }) {
  return (
    <div role="status" aria-label={label} className="flex flex-col gap-3">
      {Array.from({ length: rows }, (_, i) => (
        <Skeleton key={i} className={i === 0 ? 'h-24' : 'h-16'} />
      ))}
    </div>
  )
}

export function Modal({
  title,
  onClose,
  children,
  footer,
  wide = false,
}: {
  title: string
  onClose: () => void
  children: ReactNode
  footer?: ReactNode
  wide?: boolean
}) {
  const ref = useRef<HTMLDivElement>(null)
  const titleId = useId()

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null
    ref.current?.focus()
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', onKey)
    const overflow = document.body.style.overflow
    document.body.style.overflow = 'hidden'
    return () => {
      document.removeEventListener('keydown', onKey)
      document.body.style.overflow = overflow
      previous?.focus?.()
    }
  }, [onClose])

  return (
    <div
      className="fixed inset-0 z-50 flex items-end justify-center bg-black/45 sm:items-center sm:p-6"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        className={cx(
          'flex max-h-[92dvh] w-full flex-col rounded-t-[18px] border border-line bg-surface outline-none sm:rounded-[16px]',
          wide ? 'sm:max-w-3xl' : 'sm:max-w-xl',
        )}
      >
        <header className="flex items-center justify-between gap-3 border-b border-line px-5 py-3.5">
          <h2 id={titleId} className="text-[18px] font-semibold">
            {title}
          </h2>
          <IconButton icon="close" label="Fermer" onClick={onClose} />
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-5">{children}</div>
        {footer ? (
          <footer className="flex flex-wrap justify-end gap-2 border-t border-line px-5 py-3.5 pb-[max(0.875rem,env(safe-area-inset-bottom))]">
            {footer}
          </footer>
        ) : null}
      </div>
    </div>
  )
}

export function ConfirmModal({
  title,
  children,
  confirmLabel,
  onConfirm,
  onClose,
  busy,
  error,
}: {
  title: string
  children: ReactNode
  confirmLabel: string
  onConfirm: () => void
  onClose: () => void
  busy?: boolean
  error?: unknown
}) {
  return (
    <Modal
      title={title}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Annuler</Button>
          <Button variant="danger" busy={busy} onClick={onConfirm}>
            {confirmLabel}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <div className="text-muted">{children}</div>
        <FormError error={error} />
      </div>
    </Modal>
  )
}

/** Montant coloré selon son signe. */
export function Signed({ value, children }: { value: number | null | undefined; children: ReactNode }) {
  if (value === null || value === undefined) return <span className="text-muted">–</span>
  return <span className={cx('num', value > 0 && 'text-pos', value < 0 && 'text-neg')}>{children}</span>
}

export function Dot({ color }: { color: string }) {
  return <span className="inline-block size-2.5 shrink-0 rounded-full" style={{ background: color }} aria-hidden="true" />
}
