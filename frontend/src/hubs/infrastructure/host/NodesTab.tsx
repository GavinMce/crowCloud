import { Link, useOutletContext } from 'react-router-dom'
import type { ProviderDetail } from '../../../api/providers'
import { DataTable, type DataTableColumn } from '../../../ui/DataTable'

type NodeRow = {
  name: string
  defaultStorage: string
  defaultBridge: string
}

export function NodesTab() {
  const host = useOutletContext<ProviderDetail>()

  // Only one node exists per host until discovery ships (issue #32) — still
  // rendered as a list so the UI is already shaped for when there are more.
  const nodes: NodeRow[] = [
    {
      name: host.config.node,
      defaultStorage: host.config.default_storage,
      defaultBridge: host.config.default_bridge,
    },
  ]

  const columns: DataTableColumn<NodeRow>[] = [
    {
      key: 'name',
      header: 'Name',
      render: (row) => (
        <Link to={`/infrastructure/proxmox-hosts/${host.id}/nodes/${encodeURIComponent(row.name)}`}>
          {row.name}
        </Link>
      ),
    },
    { key: 'defaultStorage', header: 'Default storage' },
    { key: 'defaultBridge', header: 'Default bridge' },
  ]

  return (
    <div className="az-stack-col az-gap-4">
      <h2>Nodes</h2>
      <p className="az-text-secondary">
        Node discovery isn&apos;t implemented yet — this is the single node entered when the host
        was created. See{' '}
        <a href="https://github.com/GavinMce/crowCloud/issues/32" target="_blank" rel="noreferrer">
          issue #32
        </a>{' '}
        for auto-discovering every node in a cluster and configuring them individually.
      </p>
      <DataTable columns={columns} data={nodes} keyField="name" />
    </div>
  )
}
