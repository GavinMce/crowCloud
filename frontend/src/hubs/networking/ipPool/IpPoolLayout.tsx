import { Outlet, useParams } from 'react-router-dom'
import { useIpPool } from '../../../api/ipPools'
import { EntityLayout, type EntityNavEntry } from '../../../layout/EntityLayout'

const NAV_ITEMS: EntityNavEntry[] = [
  { type: 'link', to: 'overview', label: 'Overview' },
  { type: 'link', to: 'allocations', label: 'Allocations' },
  { type: 'link', to: 'activity-log', label: 'Activity log' },
]

export function IpPoolLayout() {
  const { name = '' } = useParams()
  const pool = useIpPool(name)

  if (pool.isLoading) {
    return (
      <div className="az-page">
        <p>Loading…</p>
      </div>
    )
  }

  if (pool.isError || !pool.data) {
    return (
      <div className="az-page">
        <p className="az-alert az-alert-danger">Failed to load this IP pool.</p>
      </div>
    )
  }

  return (
    <EntityLayout
      breadcrumb={[{ label: 'Networking', to: '/networking' }, { label: pool.data.name }]}
      type="IP Pool"
      name={pool.data.name}
      navItems={NAV_ITEMS}
    >
      <Outlet context={pool.data} />
    </EntityLayout>
  )
}
