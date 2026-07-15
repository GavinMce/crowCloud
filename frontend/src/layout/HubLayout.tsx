import { NavLink, Outlet } from 'react-router-dom'
import { getHub } from '../hubs/hubConfig'

function navClass({ isActive }: { isActive: boolean }) {
  return `az-hub-nav-item ${isActive ? 'az-hub-nav-item-active' : ''}`.trim()
}

export function HubLayout({ hubId }: { hubId: string }) {
  const hub = getHub(hubId)
  if (!hub) return null

  return (
    <div className="az-hub">
      <nav className="az-hub-nav">
        <div className="az-hub-nav-title">{hub.label}</div>
        <NavLink to="overview" className={navClass}>
          Overview
        </NavLink>
        <NavLink to="all-resources" className={navClass}>
          All resources
        </NavLink>
        <div className="az-hub-nav-section">Resource types</div>
        {hub.resourceTypes.map((rt) => (
          <NavLink key={rt.id} to={rt.id} className={navClass}>
            {rt.label}
          </NavLink>
        ))}
      </nav>
      <div className="az-hub-content">
        <Outlet />
      </div>
    </div>
  )
}
