import { useProviders } from '../../api/providers'
import type { ProviderRow } from '../../api/providers'
import { DataTable, type DataTableColumn } from '../../ui/DataTable'

export function AllHostsPage() {
  const providers = useProviders()
  const hosts = providers.data ?? []

  const columns: DataTableColumn<ProviderRow>[] = [
    { key: 'name', header: 'Name' },
    { key: 'provider_type', header: 'Type' },
    {
      key: 'created_at',
      header: 'Created',
      render: (row) => new Date(row.created_at).toLocaleString(),
    },
  ]

  return (
    <div className="az-page">
      <div className="az-stack-col az-gap-4">
        <h1>All resources</h1>
        {providers.isLoading && <p>Loading…</p>}
        {providers.isError && <p className="az-alert az-alert-danger">Failed to load hosts.</p>}
        {providers.data && hosts.length === 0 && <p>No hosts registered yet.</p>}
        {hosts.length > 0 && <DataTable columns={columns} data={hosts} keyField="id" />}
      </div>
    </div>
  )
}
