import { Outlet, useParams } from 'react-router-dom'
import { EntityLayout, type EntityNavEntry } from '../../layout/EntityLayout'

const NAV_ITEMS: EntityNavEntry[] = [
  { type: 'link', to: 'overview', label: 'Overview' },
  { type: 'link', to: 'monitoring', label: 'Monitoring' },
]

export function KubernetesClusterLayout() {
  const { name = '' } = useParams()

  return (
    <EntityLayout
      breadcrumb={[{ label: 'Containers', to: '/containers' }, { label: name }]}
      type="Kubernetes Cluster"
      name={name}
      navItems={NAV_ITEMS}
    >
      <Outlet />
    </EntityLayout>
  )
}
