import { Outlet, useParams } from 'react-router-dom'
import { EntityLayout, type EntityNavEntry } from '../../layout/EntityLayout'

const NAV_ITEMS: EntityNavEntry[] = [{ type: 'link', to: 'overview', label: 'Overview' }]

export function DiskLayout() {
  const { name = '' } = useParams()

  return (
    <EntityLayout
      breadcrumb={[{ label: 'Compute', to: '/compute' }, { label: name }]}
      type="Disk"
      name={name}
      navItems={NAV_ITEMS}
    >
      <Outlet />
    </EntityLayout>
  )
}
