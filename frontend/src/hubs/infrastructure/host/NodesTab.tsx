import { Link, useOutletContext } from 'react-router-dom'
import type { ProviderDetail } from '../../../api/providers'
import { useProviderNodes, type ProviderNode } from '../../../api/providerNodes'
import { DataTable, type DataTableColumn } from '../../../ui/DataTable'
import { formatCpu, formatMemory, nodeStatusVariant } from './formatNodeStats'

export function NodesTab() {
  const host = useOutletContext<ProviderDetail>()
  const nodes = useProviderNodes(host.id)

  const columns: DataTableColumn<ProviderNode>[] = [
    {
      key: 'name',
      header: 'Name',
      render: (row) => (
        <Link to={`/infrastructure/proxmox-hosts/${host.id}/nodes/${encodeURIComponent(row.name)}`}>
          {row.name}
        </Link>
      ),
    },
    {
      key: 'status',
      header: 'Status',
      render: (row) => (
        <span className={`az-pill az-pill-${nodeStatusVariant(row.status)}`}>{row.status}</span>
      ),
    },
    { key: 'cpu', header: 'CPU', render: (row) => formatCpu(row.cpu, row.max_cpu) },
    { key: 'mem', header: 'Memory', render: (row) => formatMemory(row.mem, row.max_mem) },
    {
      key: 'configured',
      header: 'Configured',
      render: (row) => (row.configured ? 'Yes' : 'No'),
    },
  ]

  return (
    <div className="az-stack-col az-gap-4">
      <h2>Nodes</h2>
      {nodes.isLoading && <p>Loading…</p>}
      {nodes.isError && <p className="az-alert az-alert-danger">Failed to load nodes.</p>}
      {nodes.data && nodes.data.length === 0 && <p>Proxmox reported no nodes for this host.</p>}
      {nodes.data && nodes.data.length > 0 && (
        <DataTable columns={columns} data={nodes.data} keyField="name" />
      )}
    </div>
  )
}
