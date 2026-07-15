import { NavLink } from 'react-router-dom'
import { HUBS } from '../hubs/hubConfig'
import { HomeIcon, ManagementIcon } from '../ui/icons'

function navClass({ isActive }: { isActive: boolean }) {
  return `az-global-nav-item ${isActive ? 'az-global-nav-item-active' : ''}`.trim()
}

// Resource hubs above the divider run on top of the hosts registered below
// it — Infrastructure (host/hardware management) belongs with Management,
// not with the services that depend on it.
const SERVICE_HUBS = HUBS.filter((hub) => hub.id !== 'infrastructure')
const INFRASTRUCTURE_HUB = HUBS.find((hub) => hub.id === 'infrastructure')

export function GlobalNav() {
  return (
    <nav className="az-global-nav">
      <NavLink to="/" end className={navClass}>
        <HomeIcon size={20} />
        Home
      </NavLink>
      {SERVICE_HUBS.map((hub) => (
        <NavLink key={hub.id} to={`/${hub.id}`} className={navClass}>
          <hub.icon size={20} />
          {hub.label}
        </NavLink>
      ))}
      <div className="az-global-nav-divider" />
      {INFRASTRUCTURE_HUB && (
        <NavLink to={`/${INFRASTRUCTURE_HUB.id}`} className={navClass}>
          <INFRASTRUCTURE_HUB.icon size={20} />
          {INFRASTRUCTURE_HUB.label}
        </NavLink>
      )}
      <NavLink to="/management/projects" className={navClass}>
        <ManagementIcon size={20} />
        Management
      </NavLink>
    </nav>
  )
}
