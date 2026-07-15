import { Outlet } from 'react-router-dom'
import { getHub } from '../hubs/hubConfig'
import { EntityLayout, type EntityNavEntry } from './EntityLayout'

export function HubLayout({ hubId }: { hubId: string }) {
  const hub = getHub(hubId)
  if (!hub) return null

  const navItems: EntityNavEntry[] = [
    { type: 'link', to: 'overview', label: 'Overview' },
    { type: 'link', to: 'all-resources', label: 'All resources' },
    { type: 'section', label: 'Resource types' },
    ...hub.resourceTypes.map((rt) => ({ type: 'link' as const, to: rt.id, label: rt.label })),
  ]

  return (
    <EntityLayout
      breadcrumb={[{ label: hub.label }]}
      type={`${hub.label} Service`}
      name={hub.label}
      navItems={navItems}
    >
      <Outlet />
    </EntityLayout>
  )
}
