import { Outlet, useParams } from 'react-router-dom'
import { useProvider } from '../../../api/providers'
import { EntityLayout, type EntityNavEntry } from '../../../layout/EntityLayout'

const NAV_ITEMS: EntityNavEntry[] = [
  { type: 'link', to: 'overview', label: 'Overview' },
  { type: 'link', to: 'nodes', label: 'Nodes' },
  { type: 'link', to: 'virtual-machines', label: 'Virtual machines' },
  { type: 'link', to: 'settings', label: 'Settings' },
  { type: 'section', label: 'Not available yet' },
  { type: 'link', to: 'storage', label: 'Storage' },
  { type: 'link', to: 'networking', label: 'Networking' },
  { type: 'link', to: 'activity-log', label: 'Activity log' },
]

export function ProxmoxHostLayout() {
  const { id = '' } = useParams()
  const host = useProvider(id)

  if (host.isLoading) {
    return (
      <div className="az-page">
        <p>Loading…</p>
      </div>
    )
  }

  if (host.isError || !host.data) {
    return (
      <div className="az-page">
        <p className="az-alert az-alert-danger">Failed to load this Proxmox host.</p>
      </div>
    )
  }

  return (
    <EntityLayout
      breadcrumb={[{ label: 'Infrastructure', to: '/infrastructure' }, { label: host.data.name }]}
      type="Proxmox Host"
      name={host.data.name}
      navItems={NAV_ITEMS}
    >
      <Outlet context={host.data} />
    </EntityLayout>
  )
}
