import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from 'react'
import { setAmountsHidden } from '@/shared/lib/format'

const KEY = 'fineme.discreet'

/** Raccourci clavier du mode discret (format `aria-keyshortcuts`). */
export const DISCREET_SHORTCUT = 'Alt+Shift+D'

type DiscreetContextValue = { hidden: boolean; toggle: () => void }

const DiscreetContext = createContext<DiscreetContextValue>({ hidden: false, toggle: () => {} })

function readStored(): boolean {
  try {
    return localStorage.getItem(KEY) === 'on'
  } catch {
    return false
  }
}

function store(hidden: boolean) {
  try {
    if (hidden) localStorage.setItem(KEY, 'on')
    else localStorage.removeItem(KEY)
  } catch {
    // préférence non mémorisée
  }
}

/** Le formateur partagé lit l'interrupteur global ; il est positionné avant le rendu qui en dépend. */
export function DiscreetProvider({ children }: { children: ReactNode }) {
  const [hidden, setHidden] = useState(() => {
    const stored = readStored()
    setAmountsHidden(stored)
    return stored
  })

  const toggle = useCallback(() => {
    setHidden((current) => {
      const next = !current
      setAmountsHidden(next)
      store(next)
      return next
    })
  }, [])

  const value = useMemo(() => ({ hidden, toggle }), [hidden, toggle])
  return <DiscreetContext.Provider value={value}>{children}</DiscreetContext.Provider>
}

export function useDiscreet() {
  return useContext(DiscreetContext)
}

/** Vrai pour Alt+Maj+D, quelle que soit la disposition du clavier (Alt modifie la lettre produite sur macOS). */
export function isDiscreetShortcut(event: Pick<KeyboardEvent, 'altKey' | 'shiftKey' | 'ctrlKey' | 'metaKey' | 'code'>) {
  return event.altKey && event.shiftKey && !event.ctrlKey && !event.metaKey && event.code === 'KeyD'
}
