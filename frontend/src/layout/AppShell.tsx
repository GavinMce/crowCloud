import { DropdownMenu, PageLayout } from '@crow-dev/ui'
import { Outlet, useLocation } from 'react-router-dom'
import { useAuth } from '../auth/useAuth'
import { RouterNavbar } from './RouterNavbar'

const NAV_LINKS = [
  { label: 'Home', href: '/' },
  { label: 'Projects', href: '/projects' },
  { label: 'Cloud Hosts', href: '/cloud-hosts' },
]

export function AppShell() {
  const location = useLocation()
  const { username, logout } = useAuth()

  const links = NAV_LINKS.map((link) => ({
    ...link,
    active:
      link.href === '/' ? location.pathname === '/' : location.pathname.startsWith(link.href),
  }))

  return (
    <PageLayout
      navbar={
        <RouterNavbar
          logo={<strong>crowCloud</strong>}
          links={links}
          actions={
            <DropdownMenu
              trigger={<button type="button">{username ?? 'Account'}</button>}
              items={[{ id: 'logout', label: 'Log out', onClick: logout }]}
              align="right"
            />
          }
        />
      }
    >
      <Outlet />
    </PageLayout>
  )
}
