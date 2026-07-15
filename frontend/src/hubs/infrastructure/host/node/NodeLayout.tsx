import { Outlet, useParams } from 'react-router-dom'
import { useProvider } from '../../../../api/providers'
import { EntityLayout, type EntityNavEntry } from '../../../../layout/EntityLayout'

const NAV_ITEMS: EntityNavEntry[] = [{ type: 'link', to: 'overview', label: 'Overview' }]

export interface NodeOutletContext {
  hostId: string
  hostName: string
  nodeName: string
}

export function NodeLayout() {
  const { id = '', nodeName = '' } = useParams()
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
        <p className="az-alert az-alert-danger">Failed to load this node.</p>
      </div>
    )
  }

  const context: NodeOutletContext = { hostId: host.data.id, hostName: host.data.name, nodeName }

  return (
    <EntityLayout
      breadcrumb={[
        { label: 'Infrastructure', to: '/infrastructure' },
        { label: host.data.name, to: `/infrastructure/proxmox-hosts/${host.data.id}` },
        { label: nodeName },
      ]}
      type="Proxmox Node"
      name={nodeName}
      navItems={NAV_ITEMS}
    >
      <Outlet context={context} />
    </EntityLayout>
  )
}
