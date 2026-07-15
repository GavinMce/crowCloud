import { Outlet } from 'react-router-dom'
import { ProjectProvider } from '../hooks/useCurrentProject'
import { GlobalNav } from './GlobalNav'
import { TopBar } from './TopBar'

export function AppShell() {
  return (
    <ProjectProvider>
      <div className="az-shell">
        <TopBar />
        <div className="az-body">
          <GlobalNav />
          <main>
            <Outlet />
          </main>
        </div>
      </div>
    </ProjectProvider>
  )
}
