import { useState } from 'react'
import type { FormEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import { useCurrentProject } from '../../hooks/useCurrentProject'
import { useProviders } from '../../api/providers'
import { useProviderNodes } from '../../api/providerNodes'
import { useIpPools } from '../../api/ipPools'
import { useCreateVm } from '../../api/resources'
import { ApiError } from '../../api/client'
import { Breadcrumb } from '../../ui/Breadcrumb'
import { Button } from '../../ui/Button'
import { Select } from '../../ui/Select'
import { Tabs } from '../../ui/Tabs'
import { TextField } from '../../ui/TextField'

const TABS = [
  { id: 'basics', label: 'Basics' },
  { id: 'networking', label: 'Networking' },
  { id: 'review', label: 'Review + create' },
]

export function CreateVirtualMachinePage() {
  const navigate = useNavigate()
  const { current } = useCurrentProject()
  const providers = useProviders()
  const createVm = useCreateVm(current ?? '')

  const [tab, setTab] = useState('basics')
  const [error, setError] = useState<string | null>(null)
  const [form, setForm] = useState({
    name: '',
    providerId: '',
    node: '',
    cpu: 2,
    memoryMib: 2048,
    diskGib: 20,
    image: '',
    ipPool: '',
  })

  const nodes = useProviderNodes(form.providerId || null)
  const configuredNodes = (nodes.data ?? []).filter((n) => n.configured)
  const ipPools = useIpPools()

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault()
    if (!current) return
    setError(null)
    try {
      await createVm.mutateAsync({
        name: form.name,
        provider_id: form.providerId,
        node: form.node,
        cpu: form.cpu,
        memory_mib: form.memoryMib,
        disk_gib: form.diskGib,
        image: form.image,
        ip_pool: form.ipPool || undefined,
      })
      navigate('/compute/virtual-machines')
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Failed to create virtual machine')
    }
  }

  if (!current) {
    return (
      <div className="az-page">
        <p className="az-text-secondary">
          Select or create a project from the top bar before creating a virtual machine.
        </p>
      </div>
    )
  }

  const providerName = providers.data?.find((p) => p.id === form.providerId)?.name
  const selectedPool = ipPools.data?.find((p) => p.name === form.ipPool)

  return (
    <div className="az-page">
      <div className="az-stack-col az-gap-4">
        <Breadcrumb
          items={[
            { label: 'Virtual machines', to: '/compute/virtual-machines' },
            { label: 'Create' },
          ]}
        />
        <h1>Create a virtual machine</h1>
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
                label="CPU"
                type="number"
                min={1}
                value={form.cpu}
                onChange={(e) => setForm({ ...form, cpu: Number(e.target.value) })}
                required
              />
              <TextField
                label="Memory (MiB)"
                type="number"
                min={1024}
                step={1024}
                value={form.memoryMib}
                onChange={(e) => setForm({ ...form, memoryMib: Number(e.target.value) })}
                required
                hint="Must be a whole number of GiB (a multiple of 1024)"
              />
              <TextField
                label="Disk (GiB)"
                type="number"
                min={1}
                value={form.diskGib}
                onChange={(e) => setForm({ ...form, diskGib: Number(e.target.value) })}
                required
              />
              <TextField
                label="Image"
                value={form.image}
                onChange={(e) => setForm({ ...form, image: e.target.value })}
                required
                hint="Proxmox template VMID"
              />
              <div>
                <Button type="button" variant="primary" onClick={() => setTab('networking')}>
                  Next: Networking
                </Button>
              </div>
            </div>
          )}

          {tab === 'networking' && (
            <div className="az-stack-col az-gap-4" style={{ maxWidth: 480 }}>
              <Select
                label="IP pool (optional)"
                value={form.ipPool}
                onChange={(e) => setForm({ ...form, ipPool: e.target.value })}
                hint="Determines both the static address and which bridge the VM's NIC attaches to. Leave unselected for DHCP on the host's default bridge."
              >
                <option value="">DHCP (node's default bridge)</option>
                {ipPools.data?.map((pool) => (
                  <option key={pool.name} value={pool.name}>
                    {pool.name} ({pool.cidr}, bridge {pool.bridge})
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
                    <strong>CPU:</strong> {form.cpu}
                  </div>
                  <div>
                    <strong>Memory:</strong> {form.memoryMib} MiB
                  </div>
                  <div>
                    <strong>Disk:</strong> {form.diskGib} GiB
                  </div>
                  <div>
                    <strong>Image:</strong> {form.image || '—'}
                  </div>
                  <div>
                    <strong>IP pool:</strong> {form.ipPool || 'DHCP'}
                  </div>
                  <div>
                    <strong>Bridge:</strong>{' '}
                    {selectedPool ? `${selectedPool.bridge} (from pool)` : "Node's default bridge"}
                  </div>
                </dl>
              </div>
              {error && <p className="az-alert az-alert-danger">{error}</p>}
              <div className="az-stack-row az-gap-2">
                <Button
                  type="submit"
                  variant="primary"
                  disabled={
                    createVm.isPending ||
                    !form.name ||
                    !form.providerId ||
                    !form.node ||
                    !form.image
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
