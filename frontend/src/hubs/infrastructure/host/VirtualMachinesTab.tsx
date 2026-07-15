import { Link, useOutletContext } from 'react-router-dom'
import type { ProviderDetail } from '../../../api/providers'
import { useCurrentProject } from '../../../hooks/useCurrentProject'
import { useResources } from '../../../api/resources'
import type { ResourceRow } from '../../../api/resources'
import { DataTable, type DataTableColumn } from '../../../ui/DataTable'
import { StatusPill } from '../../../ui/StatusPill'

export function VirtualMachinesTab() {
  const host = useOutletContext<ProviderDetail>()
  const { current } = useCurrentProject()
  const resources = useResources(current ?? '')

  const vms = (resources.data ?? []).filter(
    (r) => r.resource_type === 'vm' && r.provider_id === host.id,
  )

  const columns: DataTableColumn<ResourceRow>[] = [
    {
      key: 'name',
      header: 'Name',
      render: (row) => (
        <Link to={`/compute/virtual-machines/${encodeURIComponent(row.name)}`}>{row.name}</Link>
      ),
    },
    { key: 'phase', header: 'Status', render: (row) => <StatusPill phase={row.phase} /> },
    {
      key: 'created_at',
      header: 'Created',
      render: (row) => new Date(row.created_at).toLocaleString(),
    },
  ]

  return (
    <div className="az-stack-col az-gap-4">
      <h2>Virtual machines</h2>
      {!current && (
        <p className="az-text-secondary">
          Select a project from the top bar to see virtual machines on this host.
        </p>
      )}
      {current && vms.length === 0 && <p>No virtual machines on this host in this project.</p>}
      {current && vms.length > 0 && <DataTable columns={columns} data={vms} keyField="id" />}
    </div>
  )
}
