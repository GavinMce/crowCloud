import { useState } from 'react'
import type { FormEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import { useCurrentProject } from '../../hooks/useCurrentProject'
import { useProviders } from '../../api/providers'
import { useCreateVm } from '../../api/resources'
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
    cpu: 2,
    memoryMib: 2048,
    diskGib: 20,
    image: '',
    ipPool: '',
  })

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault()
    if (!current) return
    setError(null)
    try {
      await createVm.mutateAsync({
        name: form.name,
        provider_id: form.providerId,
        cpu: form.cpu,
        memory_mib: form.memoryMib,
        disk_gib: form.diskGib,
        image: form.image,
        ip_pool: form.ipPool.trim() ? form.ipPool.trim() : undefined,
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
                onChange={(e) => setForm({ ...form, providerId: e.target.value })}
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
              <TextField
                label="IP pool (optional)"
                value={form.ipPool}
                onChange={(e) => setForm({ ...form, ipPool: e.target.value })}
                hint="Name of an IpPool to request a static address from. Leave blank for DHCP."
              />
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
                </dl>
              </div>
              {error && <p className="az-alert az-alert-danger">{error}</p>}
              <div className="az-stack-row az-gap-2">
                <Button
                  type="submit"
                  variant="primary"
                  disabled={createVm.isPending || !form.name || !form.providerId || !form.image}
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
