import { useState } from 'react'
import type { FormEvent } from 'react'
import {
  type ProviderRow,
  useCreateProvider,
  useDeleteProvider,
  useProviders,
} from '../../api/providers'
import { ApiError } from '../../api/client'
import { Button } from '../../ui/Button'
import { CommandBar } from '../../ui/CommandBar'
import { DataTable, type DataTableColumn } from '../../ui/DataTable'
import { Modal } from '../../ui/Modal'
import { Select } from '../../ui/Select'
import { TextField } from '../../ui/TextField'
import { useAuth } from '../../auth/useAuth'

const PROVIDER_TYPES = [
  { value: 'proxmox', label: 'Proxmox', enabled: true },
  { value: 'opnsense', label: 'OPNsense', enabled: false },
  { value: 'hetzner', label: 'Hetzner', enabled: false },
  { value: 'cloudflare', label: 'Cloudflare', enabled: false },
]

const EMPTY_PROXMOX_FORM = {
  name: '',
  url: '',
  tokenId: '',
  tokenSecret: '',
  node: '',
  defaultStorage: '',
  defaultBridge: '',
  tlsInsecure: false,
}

export function CloudHostsPage() {
  const { isAdmin } = useAuth()
  const providers = useProviders()
  const createProvider = useCreateProvider()
  const deleteProvider = useDeleteProvider()

  const [createOpen, setCreateOpen] = useState(false)
  const [providerType, setProviderType] = useState('proxmox')
  const [form, setForm] = useState(EMPTY_PROXMOX_FORM)
  const [error, setError] = useState<string | null>(null)
  const [pendingDelete, setPendingDelete] = useState<ProviderRow | null>(null)

  const handleCreate = async (e: FormEvent) => {
    e.preventDefault()
    setError(null)
    try {
      await createProvider.mutateAsync({
        name: form.name,
        provider_type: providerType,
        config: {
          url: form.url,
          token_id: form.tokenId,
          token_secret: form.tokenSecret,
          node: form.node,
          default_storage: form.defaultStorage,
          default_bridge: form.defaultBridge,
          tls_insecure: form.tlsInsecure,
        },
      })
      setCreateOpen(false)
      setForm(EMPTY_PROXMOX_FORM)
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Failed to add cloud host')
    }
  }

  const handleDelete = async () => {
    if (!pendingDelete) return
    await deleteProvider.mutateAsync(pendingDelete.id)
    setPendingDelete(null)
  }

  const columns: DataTableColumn<ProviderRow>[] = [
    { key: 'name', header: 'Name' },
    { key: 'provider_type', header: 'Type' },
    {
      key: 'created_at',
      header: 'Created',
      render: (row) => new Date(row.created_at).toLocaleString(),
    },
    ...(isAdmin
      ? [
          {
            key: 'id',
            header: '',
            render: (row: ProviderRow) => (
              <Button variant="ghost" size="sm" onClick={() => setPendingDelete(row)}>
                Delete
              </Button>
            ),
          },
        ]
      : []),
  ]

  return (
    <div className="az-page">
      <div className="az-stack-col az-gap-4">
        <h1>Cloud hosts</h1>
        <CommandBar>
          {isAdmin ? (
            <Button variant="primary" onClick={() => setCreateOpen(true)}>
              + Create
            </Button>
          ) : (
            <p className="az-text-secondary">Only admins can add cloud hosts.</p>
          )}
        </CommandBar>

        {providers.isLoading && <p>Loading…</p>}
        {providers.isError && <p className="az-alert az-alert-danger">Failed to load cloud hosts.</p>}
        {providers.data && providers.data.length === 0 && <p>No cloud hosts configured yet.</p>}
        {providers.data && providers.data.length > 0 && (
          <DataTable columns={columns} data={providers.data} keyField="id" />
        )}
      </div>

      <Modal open={createOpen} title="Add cloud host" onClose={() => setCreateOpen(false)}>
        <form onSubmit={handleCreate}>
          <div className="az-stack-col az-gap-4">
            <Select
              label="Type"
              value={providerType}
              onChange={(e) => setProviderType(e.target.value)}
            >
              {PROVIDER_TYPES.map((t) => (
                <option key={t.value} value={t.value} disabled={!t.enabled}>
                  {t.label}
                  {t.enabled ? '' : ' (coming soon)'}
                </option>
              ))}
            </Select>
            <TextField
              label="Name"
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
              required
              autoFocus
            />
            <TextField
              label="URL"
              placeholder="https://pve.example.com:8006"
              value={form.url}
              onChange={(e) => setForm({ ...form, url: e.target.value })}
              required
            />
            <TextField
              label="Token ID"
              placeholder="root@pam!crow"
              value={form.tokenId}
              onChange={(e) => setForm({ ...form, tokenId: e.target.value })}
              required
            />
            <TextField
              label="Token Secret"
              type="password"
              value={form.tokenSecret}
              onChange={(e) => setForm({ ...form, tokenSecret: e.target.value })}
              required
            />
            <TextField
              label="Node"
              value={form.node}
              onChange={(e) => setForm({ ...form, node: e.target.value })}
              required
            />
            <TextField
              label="Default Storage"
              value={form.defaultStorage}
              onChange={(e) => setForm({ ...form, defaultStorage: e.target.value })}
              required
            />
            <TextField
              label="Default Bridge"
              value={form.defaultBridge}
              onChange={(e) => setForm({ ...form, defaultBridge: e.target.value })}
              required
            />
            <label className="az-stack-row az-gap-2">
              <input
                type="checkbox"
                checked={form.tlsInsecure}
                onChange={(e) => setForm({ ...form, tlsInsecure: e.target.checked })}
              />
              Allow insecure TLS
            </label>
            {error && <p className="az-alert az-alert-danger">{error}</p>}
            <Button type="submit" variant="primary" disabled={createProvider.isPending}>
              Add
            </Button>
          </div>
        </form>
      </Modal>

      <Modal
        open={pendingDelete !== null}
        title="Delete cloud host"
        onClose={() => setPendingDelete(null)}
      >
        <div className="az-stack-col az-gap-4">
          <p>
            Delete cloud host <strong>{pendingDelete?.name}</strong>? This cannot be undone.
          </p>
          <div className="az-stack-row az-gap-2">
            <Button variant="primary" onClick={handleDelete} disabled={deleteProvider.isPending}>
              Delete
            </Button>
            <Button variant="default" onClick={() => setPendingDelete(null)}>
              Cancel
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  )
}
