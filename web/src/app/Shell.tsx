import { useEffect, useState } from 'react'
import { NavLink, Outlet, useLocation } from 'react-router'
import { useMembers } from '@/shared/api/queries'
import { DISCREET_SHORTCUT, isDiscreetShortcut, useDiscreet } from '@/shared/state/discreet'
import { useView } from '@/shared/state/view'
import { ErrorBoundary } from '@/shared/ui/ErrorBoundary'
import { OfflineNotice } from '@/shared/ui/OfflineNotice'
import { Icon, Logo, type IconName } from '@/shared/ui/Icon'
import { cx } from '@/shared/ui/ui'

type NavItem = { to: string; label: string; short?: string; icon: IconName; end?: boolean }

const NAV: NavItem[] = [
  { to: '/', label: 'Vue d’ensemble', short: 'Synthèse', icon: 'overview', end: true },
  { to: '/analyse', label: 'Analyse', icon: 'analysis' },
  { to: '/comptes', label: 'Comptes', icon: 'accounts' },
  { to: '/immobilier', label: 'Immobilier', icon: 'home' },
  { to: '/objectifs', label: 'Objectifs', icon: 'goal' },
  { to: '/projection', label: 'Projection', icon: 'projection' },
  { to: '/frais', label: 'Simulateur de frais', icon: 'fees' },
  { to: '/connexions', label: 'Connexions', icon: 'link' },
  { to: '/parametres', label: 'Paramètres', icon: 'settings' },
]

const MOBILE_PRIMARY = ['/', '/comptes', '/objectifs', '/projection']

function ViewSwitcher({ compact = false }: { compact?: boolean }) {
  const { memberId, setMemberId } = useView()
  const members = useMembers()
  const list = members.data ?? []
  const current = list.find((member) => member.id === memberId)

  // Un membre supprimé ne doit pas rester sélectionné.
  useEffect(() => {
    if (memberId && members.data && !members.data.some((member) => member.id === memberId)) setMemberId(null)
  }, [memberId, members.data, setMemberId])

  return (
    <label className={cx('relative flex items-center', compact ? 'max-w-[11rem]' : 'w-full')}>
      <span className="sr-only">Vue affichée</span>
      <span
        className="pointer-events-none absolute left-3 size-2.5 rounded-full"
        style={{ background: current?.color ?? 'var(--brass)' }}
        aria-hidden="true"
      />
      <select
        value={current ? current.id : ''}
        onChange={(event) => setMemberId(event.target.value || null)}
        className="min-h-10 w-full appearance-none truncate rounded-[9px] border border-line bg-surface py-1.5 pr-8 pl-8 text-[14.5px] font-medium hover:border-muted"
      >
        <option value="">Foyer</option>
        {list.map((member) => (
          <option key={member.id} value={member.id}>
            {member.name}
          </option>
        ))}
      </select>
      <Icon name="chevron" size={15} className="pointer-events-none absolute right-2.5 rotate-90 text-muted" />
    </label>
  )
}

/** Masque ou affiche tous les montants (mode discret), aussi par Alt+Maj+D. */
function DiscreetToggle({ withLabel = false }: { withLabel?: boolean }) {
  const { hidden, toggle } = useDiscreet()
  return (
    <button
      type="button"
      aria-pressed={hidden}
      aria-keyshortcuts={DISCREET_SHORTCUT}
      aria-label={withLabel ? undefined : 'Masquer les montants'}
      title={`Masquer les montants (${DISCREET_SHORTCUT.replace('Shift', 'Maj')})`}
      onClick={toggle}
      className={cx(
        'inline-flex min-h-10 shrink-0 items-center gap-3 rounded-[9px] text-[14.5px] transition',
        withLabel ? 'px-3' : 'size-10 justify-center',
        hidden ? 'bg-accent-soft font-semibold text-accent' : 'text-muted hover:bg-surface-2 hover:text-ink',
      )}
    >
      <Icon name={hidden ? 'eye-off' : 'eye'} size={withLabel ? 18 : 20} />
      {withLabel ? 'Masquer les montants' : null}
    </button>
  )
}

function useDiscreetShortcut() {
  const { toggle } = useDiscreet()
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!isDiscreetShortcut(event) || event.repeat) return
      event.preventDefault()
      toggle()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [toggle])
}

function navClass({ isActive }: { isActive: boolean }) {
  return cx(
    'flex min-h-10 items-center gap-3 rounded-[9px] px-3 text-[14.5px] transition',
    isActive ? 'bg-accent-soft font-semibold text-accent' : 'text-muted hover:bg-surface-2 hover:text-ink',
  )
}

export function Shell() {
  const [menuOpen, setMenuOpen] = useState(false)
  const location = useLocation()
  useDiscreetShortcut()

  useEffect(() => {
    window.scrollTo(0, 0)
  }, [location.pathname])

  const primary = NAV.filter((item) => MOBILE_PRIMARY.includes(item.to))
  const secondary = NAV.filter((item) => !MOBILE_PRIMARY.includes(item.to))
  const secondaryActive = secondary.some((item) => location.pathname.startsWith(item.to))

  return (
    <div className="min-h-dvh lg:pl-[248px]">
      <a
        href="#contenu"
        className="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-50 focus:rounded-lg focus:bg-surface focus:px-3 focus:py-2"
      >
        Aller au contenu
      </a>

      {/* Navigation latérale (ordinateur) */}
      <aside className="fixed inset-y-0 left-0 hidden w-[248px] flex-col gap-5 border-r border-line bg-surface px-4 py-5 lg:flex">
        <NavLink to="/" className="flex items-center gap-2.5 px-1" aria-label="FineMe, vue d’ensemble">
          <Logo />
          <span className="font-display text-[21px] font-semibold tracking-tight">FineMe</span>
        </NavLink>
        <ViewSwitcher />
        <nav aria-label="Navigation principale" className="flex flex-col gap-0.5">
          {NAV.map((item) => (
            <NavLink key={item.to} to={item.to} end={item.end} className={navClass}>
              <Icon name={item.icon} />
              {item.label}
            </NavLink>
          ))}
        </nav>
        <div className="mt-auto">
          <DiscreetToggle withLabel />
        </div>
      </aside>

      {/* Dans la colonne de contenu : jamais sous la barre latérale. */}
      <OfflineNotice />

      {/* Barre supérieure (téléphone) */}
      <header className="sticky top-0 z-30 flex items-center justify-between gap-3 border-b border-line bg-surface/95 px-4 py-2.5 backdrop-blur lg:hidden">
        <NavLink to="/" className="flex items-center gap-2" aria-label="FineMe, vue d’ensemble">
          <Logo size={26} />
          <span className="font-display text-[19px] font-semibold tracking-tight">FineMe</span>
        </NavLink>
        <div className="flex items-center gap-1">
          <DiscreetToggle />
          <ViewSwitcher compact />
        </div>
      </header>

      <main id="contenu" className="mx-auto w-full max-w-[1180px] px-4 pt-5 pb-28 sm:px-6 lg:px-9 lg:pt-9 lg:pb-14">
        <ErrorBoundary key={location.pathname}>
          <Outlet />
        </ErrorBoundary>
      </main>

      {/* Barre d'onglets (téléphone) */}
      <nav
        aria-label="Navigation principale"
        className="fixed inset-x-0 bottom-0 z-30 grid grid-cols-5 border-t border-line bg-surface/95 pb-[env(safe-area-inset-bottom)] backdrop-blur lg:hidden"
      >
        {primary.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.end}
            onClick={() => setMenuOpen(false)}
            className={({ isActive }) =>
              cx(
                'flex min-h-[58px] flex-col items-center justify-center gap-0.5 text-[11.5px]',
                isActive ? 'font-semibold text-accent' : 'text-muted',
              )
            }
          >
            <Icon name={item.icon} size={21} />
            {item.short ?? item.label}
          </NavLink>
        ))}
        <button
          type="button"
          aria-expanded={menuOpen}
          aria-controls="menu-plus"
          onClick={() => setMenuOpen((open) => !open)}
          className={cx(
            'flex min-h-[58px] flex-col items-center justify-center gap-0.5 text-[11.5px]',
            menuOpen || secondaryActive ? 'font-semibold text-accent' : 'text-muted',
          )}
        >
          <Icon name="more" size={21} />
          Plus
        </button>
      </nav>

      {menuOpen ? (
        <div className="fixed inset-0 z-20 bg-black/40 lg:hidden" onClick={() => setMenuOpen(false)}>
          <nav
            id="menu-plus"
            aria-label="Autres écrans"
            onClick={(event) => event.stopPropagation()}
            className="absolute inset-x-0 bottom-[calc(58px+env(safe-area-inset-bottom))] flex flex-col gap-0.5 rounded-t-[18px] border-t border-line bg-surface p-3"
          >
            {secondary.map((item) => (
              <NavLink key={item.to} to={item.to} className={navClass} onClick={() => setMenuOpen(false)}>
                <Icon name={item.icon} />
                {item.label}
              </NavLink>
            ))}
          </nav>
        </div>
      ) : null}
    </div>
  )
}
