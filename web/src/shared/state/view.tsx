import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from 'react'

const KEY = 'fineme.view'

type ViewContextValue = {
  /** null = vue foyer ; sinon identifiant du membre affiché. */
  memberId: string | null
  setMemberId: (id: string | null) => void
}

const ViewContext = createContext<ViewContextValue>({ memberId: null, setMemberId: () => {} })

function readStored(): string | null {
  try {
    return localStorage.getItem(KEY)
  } catch {
    return null
  }
}

export function ViewProvider({ children }: { children: ReactNode }) {
  const [memberId, setState] = useState<string | null>(readStored)

  const setMemberId = useCallback((id: string | null) => {
    setState(id)
    try {
      if (id) localStorage.setItem(KEY, id)
      else localStorage.removeItem(KEY)
    } catch {
      // préférence non mémorisée
    }
  }, [])

  const value = useMemo(() => ({ memberId, setMemberId }), [memberId, setMemberId])
  return <ViewContext.Provider value={value}>{children}</ViewContext.Provider>
}

export function useView() {
  return useContext(ViewContext)
}
