import { useQueryClient } from '@tanstack/react-query'
import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react'
import { getToken, setToken, UNAUTHORIZED_EVENT } from '@/shared/api/client'
import { purgePersistedCache } from '@/shared/api/persistence'
import type { AuthResult, User } from '@/shared/api/types'

const USER_KEY = 'fineme.user'

type AuthContextValue = {
  isAuthenticated: boolean
  user: User | null
  signIn: (result: AuthResult) => void
  /** Remplace le jeton de la session en cours (après un changement de mot de passe), sans vider le cache. */
  renewSession: (result: AuthResult) => void
  signOut: () => void
}

const AuthContext = createContext<AuthContextValue>({
  isAuthenticated: false,
  user: null,
  signIn: () => {},
  renewSession: () => {},
  signOut: () => {},
})

function readUser(): User | null {
  try {
    const raw = localStorage.getItem(USER_KEY)
    return raw ? (JSON.parse(raw) as User) : null
  } catch {
    return null
  }
}

function storeUser(user: User | null) {
  try {
    if (user) localStorage.setItem(USER_KEY, JSON.stringify(user))
    else localStorage.removeItem(USER_KEY)
  } catch {
    // stockage indisponible
  }
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient()
  const [token, setTokenState] = useState<string | null>(getToken)
  const [user, setUser] = useState<User | null>(readUser)

  const signOut = useCallback(() => {
    setToken(null)
    storeUser(null)
    setTokenState(null)
    setUser(null)
    queryClient.clear()
    // La copie hors ligne des données ne survit pas à la session.
    purgePersistedCache()
  }, [queryClient])

  const renewSession = useCallback((result: AuthResult) => {
    setToken(result.token)
    storeUser(result.user)
    setTokenState(result.token)
    setUser(result.user)
  }, [])

  const signIn = useCallback(
    (result: AuthResult) => {
      queryClient.clear()
      purgePersistedCache()
      renewSession(result)
    },
    [queryClient, renewSession],
  )

  // Le client API vide le jeton sur un 401 ; on synchronise l'état pour revenir à la connexion.
  useEffect(() => {
    window.addEventListener(UNAUTHORIZED_EVENT, signOut)
    return () => window.removeEventListener(UNAUTHORIZED_EVENT, signOut)
  }, [signOut])

  const value = useMemo(
    () => ({ isAuthenticated: Boolean(token), user, signIn, renewSession, signOut }),
    [token, user, signIn, renewSession, signOut],
  )
  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>
}

export function useAuth() {
  return useContext(AuthContext)
}
