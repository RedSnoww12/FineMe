import { lazy, Suspense } from 'react'
import { Link, Navigate, Route, Routes, useLocation } from 'react-router'
import { Shell } from './Shell'
import { EmptyState, LoadingBlock } from '@/shared/ui/ui'
import { OfflineNotice } from '@/shared/ui/OfflineNotice'
import { WakeNotice } from '@/shared/ui/WakeNotice'
import AuthPage from '@/features/auth/pages/AuthPage'
import { useAuth } from '@/shared/state/auth'

const OverviewPage = lazy(() => import('@/features/overview/pages/OverviewPage'))
const AnalysisPage = lazy(() => import('@/features/analysis/pages/AnalysisPage'))
const AccountsPage = lazy(() => import('@/features/accounts/pages/AccountsPage'))
const AccountDetailPage = lazy(() => import('@/features/accounts/pages/AccountDetailPage'))
const RealEstatePage = lazy(() => import('@/features/real-estate/pages/RealEstatePage'))
const GoalsPage = lazy(() => import('@/features/goals/pages/GoalsPage'))
const ProjectionPage = lazy(() => import('@/features/projection/pages/ProjectionPage'))
const FeesPage = lazy(() => import('@/features/fees/pages/FeesPage'))
const ConnectionsPage = lazy(() => import('@/features/connections/pages/ConnectionsPage'))
const SettingsPage = lazy(() => import('@/features/settings/pages/SettingsPage'))
const HistoryCleanupPage = lazy(() => import('@/features/settings/pages/HistoryCleanupPage'))

function Protected() {
  const { isAuthenticated } = useAuth()
  const location = useLocation()
  if (!isAuthenticated) return <Navigate to="/connexion" replace state={{ from: location.pathname }} />
  return <Shell />
}

function NotFound() {
  return (
    <EmptyState title="Cette page n’existe pas" action={<Link className="font-medium text-accent underline" to="/">Revenir à la vue d’ensemble</Link>}>
      L’adresse saisie ne correspond à aucun écran de FineMe.
    </EmptyState>
  )
}

export default function App() {
  return (
    <>
      <OfflineNotice />
      <WakeNotice />
      <Suspense fallback={<div className="p-6"><LoadingBlock /></div>}>
        <Routes>
          <Route path="/connexion" element={<AuthPage />} />
          <Route element={<Protected />}>
            <Route index element={<OverviewPage />} />
            <Route path="analyse" element={<AnalysisPage />} />
            <Route path="comptes" element={<AccountsPage />} />
            <Route path="comptes/:id" element={<AccountDetailPage />} />
            <Route path="immobilier" element={<RealEstatePage />} />
            <Route path="objectifs" element={<GoalsPage />} />
            <Route path="projection" element={<ProjectionPage />} />
            <Route path="frais" element={<FeesPage />} />
            <Route path="connexions" element={<ConnectionsPage />} />
            <Route path="parametres" element={<SettingsPage />} />
            <Route path="parametres/historique" element={<HistoryCleanupPage />} />
            <Route path="*" element={<NotFound />} />
          </Route>
        </Routes>
      </Suspense>
    </>
  )
}
