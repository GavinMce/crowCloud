import { NavLink } from 'react-router-dom'
import { HUBS } from '../hubs/hubConfig'
import { HomeIcon, ManagementIcon } from '../ui/icons'

function navClass({ isActive }: { isActive: boolean }) {
  return `az-global-nav-item ${isActive ? 'az-global-nav-item-active' : ''}`.trim()
}

export function GlobalNav() {
  return (
    <nav className="az-global-nav">
      <NavLink to="/" end className={navClass}>
        <HomeIcon size={20} />
        Home
      </NavLink>
      {HUBS.map((hub) => (
        <NavLink key={hub.id} to={`/${hub.id}`} className={navClass}>
          <hub.icon size={20} />
          {hub.label}
        </NavLink>
      ))}
      <NavLink to="/management/projects" className={navClass}>
        <ManagementIcon size={20} />
        Management
      </NavLink>
    </nav>
  )
}
