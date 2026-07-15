import { useState } from 'react'
import { Link } from 'react-router-dom'
import { useAuth } from '../auth/useAuth'
import { useCurrentProject } from '../hooks/useCurrentProject'
import { AccountIcon, ChevronDownIcon, SearchIcon } from '../ui/icons'

export function TopBar() {
  const { username, logout } = useAuth()
  const { projectNames, current, setCurrent } = useCurrentProject()
  const [projectMenuOpen, setProjectMenuOpen] = useState(false)
  const [accountMenuOpen, setAccountMenuOpen] = useState(false)

  return (
    <header className="az-topbar">
      <Link to="/" className="az-topbar-brand">
        crowCloud
      </Link>

      <div className="az-topbar-search">
        <SearchIcon size={16} />
        <input placeholder="Search resources (not yet available)" disabled />
      </div>

      <div className="az-topbar-spacer" />

      <div className="az-project-picker">
        <button
          type="button"
          className="az-project-picker-btn"
          onClick={() => setProjectMenuOpen((v) => !v)}
        >
          {current ?? 'Select a project'}
          <ChevronDownIcon size={14} />
        </button>
        {projectMenuOpen && (
          <div className="az-project-picker-menu">
            {projectNames.length === 0 && (
              <div className="az-project-picker-item az-text-secondary">No projects yet</div>
            )}
            {projectNames.map((name) => (
              <button
                key={name}
                type="button"
                className={`az-project-picker-item ${name === current ? 'az-project-picker-item-active' : ''}`.trim()}
                onClick={() => {
                  setCurrent(name)
                  setProjectMenuOpen(false)
                }}
              >
                {name}
              </button>
            ))}
            <Link
              to="/management/projects"
              className="az-project-picker-item"
              onClick={() => setProjectMenuOpen(false)}
            >
              Manage projects…
            </Link>
          </div>
        )}
      </div>

      <div className="az-project-picker">
        <button
          type="button"
          className="az-topbar-account"
          onClick={() => setAccountMenuOpen((v) => !v)}
        >
          <AccountIcon size={18} />
          {username}
        </button>
        {accountMenuOpen && (
          <div className="az-project-picker-menu">
            <button type="button" className="az-project-picker-item" onClick={logout}>
              Log out
            </button>
          </div>
        )}
      </div>
    </header>
  )
}
