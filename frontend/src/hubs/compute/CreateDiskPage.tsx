import { useState } from 'react'
import type { FormEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import { useCurrentProject } from '../../hooks/useCurrentProject'
import { useProviders } from '../../api/providers'
import { useProviderNodes } from '../../api/providerNodes'
import { useResources } from '../../api/resources'
import { useCreateDisk } from '../../api/disks'
import { ApiError } from '../../api/client'
import { Breadcrumb } from '../../ui/Breadcrumb'
import { Button } from '../../ui/Button'
import { Select } from '../../ui/Select'
import { Tabs } from '../../ui/Tabs'
import { TextField } from '../../ui/TextField'

const TABS = [
  { id: 'basics', label: 'Basics' },
  { id: 'review', label: 'Review + create' },
]

export function CreateDiskPage() {
  const navigate = useNavigate()
  const { current } = useCurrentProject()
  const providers = useProviders()
  const resources = useResources(current ?? '')
  const createDisk = useCreateDisk(current ?? '')

  const [tab, setTab] = useState('basics')
  const [error, setError] = useState<string | null>(null)
  const [form, setForm] = useState({
    name: '',
    providerId: '',
    node: '',
    sizeGib: 20,
    vmName: '',
  })

  const nodes = useProviderNodes(form.providerId || null)
  const configuredNodes = (nodes.data ?? []).filter((n) => n.configured)
  const vms = (resources.data ?? []).filter((r) => r.resource_type === 'vm')

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault()
    if (!current) return
    setError(null)
    try {
      await createDisk.mutateAsync({
        name: form.name,
        provider_id: form.providerId,
        node: form.node,
        size_gib: form.sizeGib,
        vm_name: form.vmName || undefined,
      })
      navigate('/compute/disks')
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Failed to create disk')
    }
  }

  if (!current) {
    return (
      <div className="az-page">
        <p className="az-text-secondary">
          Select or create a project from the top bar before creating a disk.
        </p>
      </div>
    )
  }

  const providerName = providers.data?.find((p) => p.id === form.providerId)?.name

  return (
    <div className="az-page">
      <div className="az-stack-col az-gap-4">
        <Breadcrumb items={[{ label: 'Disks', to: '/compute/disks' }, { label: 'Create' }]} />
        <h1>Create a disk</h1>
        <Tabs tabs={TABS} activeTab={tab} onChange={setTab} />

        <form onSubmit={handleSubmit}>
          {tab === 'basics' && (
            <div className="az-stack-col az-gap-4" style={{ maxWidth: 480 }}>
              <p className="az-text-secondary">Project: {current}</p>
              <TextField
                label="Name"
                value={form.name}
                onChange={(e) => setForm({ ...form, name: e.target.value })}
                required
                autoFocus
              />
              <Select
                label="Cloud host"
                value={form.providerId}
                onChange={(e) => setForm({ ...form, providerId: e.target.value, node: '' })}
                required
                hint="The disk's storage lives on this host's node — it can only later attach to a VM on the same node."
              >
                <option value="" disabled>
                  Select a cloud host
                </option>
                {providers.data?.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </Select>
              <Select
                label="Node"
                value={form.node}
                onChange={(e) => setForm({ ...form, node: e.target.value })}
                required
                disabled={!form.providerId}
                hint={
                  form.providerId && !nodes.isLoading && configuredNodes.length === 0
                    ? "This host has no adopted nodes yet — configure one from its Nodes tab first."
                    : undefined
                }
              >
                <option value="" disabled>
                  {form.providerId ? 'Select a node' : 'Select a cloud host first'}
                </option>
                {configuredNodes.map((n) => (
                  <option key={n.name} value={n.name}>
                    {n.name} ({n.status})
                  </option>
                ))}
              </Select>
              <TextField
                label="Size (GiB)"
                type="number"
                min={1}
                value={form.sizeGib}
                onChange={(e) => setForm({ ...form, sizeGib: Number(e.target.value) })}
                required
              />
              <Select
                label="Attach to VM (optional)"
                value={form.vmName}
                onChange={(e) => setForm({ ...form, vmName: e.target.value })}
                hint="Leave unselected to create an unattached disk you can assign later. The VM must be on the same node selected above."
              >
                <option value="">Don't attach yet</option>
                {vms.map((vm) => (
                  <option key={vm.id} value={vm.name}>
                    {vm.name}
                  </option>
                ))}
              </Select>
              <div>
                <Button type="button" variant="primary" onClick={() => setTab('review')}>
                  Next: Review + create
                </Button>
              </div>
            </div>
          )}

          {tab === 'review' && (
            <div className="az-stack-col az-gap-4" style={{ maxWidth: 480 }}>
              <div className="az-card">
                <dl className="az-stack-col az-gap-2">
                  <div>
                    <strong>Project:</strong> {current}
                  </div>
                  <div>
                    <strong>Name:</strong> {form.name || '—'}
                  </div>
                  <div>
                    <strong>Cloud host:</strong> {providerName ?? '—'}
                  </div>
                  <div>
                    <strong>Node:</strong> {form.node || '—'}
                  </div>
                  <div>
                    <strong>Size:</strong> {form.sizeGib} GiB
                  </div>
                  <div>
                    <strong>Attach to:</strong> {form.vmName || 'Unattached'}
                  </div>
                </dl>
              </div>
              {error && <p className="az-alert az-alert-danger">{error}</p>}
              <div className="az-stack-row az-gap-2">
                <Button
                  type="submit"
                  variant="primary"
                  disabled={
                    createDisk.isPending || !form.name || !form.providerId || !form.node
                  }
                >
                  Create
                </Button>
                <Button type="button" variant="default" onClick={() => setTab('basics')}>
                  Back
                </Button>
              </div>
            </div>
          )}
        </form>
      </div>
    </div>
  )
}
