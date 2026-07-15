import type { ReactNode } from 'react'
import { NavLink } from 'react-router-dom'
import { Breadcrumb, type BreadcrumbItem } from '../ui/Breadcrumb'

function navClass({ isActive }: { isActive: boolean }) {
  return `az-hub-nav-item ${isActive ? 'az-hub-nav-item-active' : ''}`.trim()
}

export type EntityNavEntry =
  | { type: 'link'; to: string; label: string }
  | { type: 'section'; label: string }

interface EntityLayoutProps {
  /** Segments after Home — EntityLayout prepends {label:'Home', to:'/'} itself. */
  breadcrumb: BreadcrumbItem[]
  /** e.g. "Infrastructure Service", "Proxmox Host", "Virtual Machine" */
  type: string
  name: string
  navItems?: EntityNavEntry[]
  children: ReactNode
}

/**
 * Shell shared by every "entity" page in the app — a service hub and a
 * specific resource are the same kind of thing (a named entity reachable
 * via breadcrumb, with a type, a side-menu, and content), so both render
 * through this one component rather than each hand-rolling their own
 * header/nav shape. The side-nav here *replaces* whatever nav was showing
 * one level up (never stacks with it) — see App.tsx's routing, which keeps
 * only list-browsing routes nested inside a parent entity's Outlet.
 *
 * No command bar here: Azure's own command bar is page-specific, not
 * entity-wide (an Overview page shows Delete/Start/Stop, a Networking
 * settings page shows Troubleshoot/Manage instead — confirmed against
 * real Azure Portal markup). Each tab's own content renders its own
 * `CommandBar` with whatever actions are relevant to just that page.
 */
export function EntityLayout({ breadcrumb, type, name, navItems = [], children }: EntityLayoutProps) {
  return (
    <div className="az-entity-page">
      <div className="az-entity-header">
        <div className="az-stack-col az-gap-2">
          <Breadcrumb items={[{ label: 'Home', to: '/' }, ...breadcrumb]} />
          <div>
            <div className="az-entity-type">{type}</div>
            <h1>{name}</h1>
          </div>
        </div>
      </div>

      {navItems.length > 0 ? (
        <div className="az-entity-layout">
          <nav className="az-entity-nav">
            {navItems.map((item) =>
              item.type === 'section' ? (
                <div key={item.label} className="az-hub-nav-section">
                  {item.label}
                </div>
              ) : (
                <NavLink key={item.to} to={item.to} className={navClass}>
                  {item.label}
                </NavLink>
              ),
            )}
          </nav>
          <div className="az-entity-content">{children}</div>
        </div>
      ) : (
        <div className="az-page">{children}</div>
      )}
    </div>
  )
}
