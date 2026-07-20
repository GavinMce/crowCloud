import { useState } from 'react'
import type { FormEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import { useCurrentProject } from '../../hooks/useCurrentProject'
import { useProviders } from '../../api/providers'
import { useProviderNodes } from '../../api/providerNodes'
import { useIpPools } from '../../api/ipPools'
import { useCreateK8sCluster } from '../../api/resources'
import { ApiError } from '../../api/client'
import { Breadcrumb } from '../../ui/Breadcrumb'
import { Button } from '../../ui/Button'
import { Select } from '../../ui/Select'
import { Tabs } from '../../ui/Tabs'
import { TextField } from '../../ui/TextField'

const TABS = [
  { id: 'basics', label: 'Basics' },
  { id: 'sizing', label: 'Cluster sizing' },
  { id: 'networking', label: 'Networking' },
  { id: 'review', label: 'Review + create' },
]

export function CreateKubernetesClusterPage() {
  const navigate = useNavigate()
  const { current } = useCurrentProject()
  const providers = useProviders()
  const createCluster = useCreateK8sCluster(current ?? '')

  const [tab, setTab] = useState('basics')
  const [error, setError] = useState<string | null>(null)
  const [form, setForm] = useState({
    name: '',
    providerId: '',
    node: '',
    image: '',
    k3sVersion: '',
    controlPlaneCpu: 2,
    controlPlaneMemoryGib: 4,
    controlPlaneDiskGib: 40,
    workerCount: 2,
    workerCpu: 2,
    workerMemoryGib: 4,
    workerDiskGib: 40,
    ipPool: '',
    podCidr: '10.42.0.0/16',
    serviceCidr: '10.43.0.0/16',
    lbPoolCidr: '',
    monitoring: false,
  })

  const nodes = useProviderNodes(form.providerId || null)
  const configuredNodes = (nodes.data ?? []).filter((n) => n.configured)
  const ipPools = useIpPools()

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault()
    if (!current) return
    setError(null)
    try {
      await createCluster.mutateAsync({
        name: form.name,
        provider_id: form.providerId,
        node: form.node,
        image: form.image,
        ip_pool: form.ipPool,
        k3s_version: form.k3sVersion || undefined,
        control_plane_cpu: form.controlPlaneCpu,
        control_plane_memory_gib: form.controlPlaneMemoryGib,
        control_plane_disk_gib: form.controlPlaneDiskGib,
        worker_count: form.workerCount,
        worker_cpu: form.workerCpu,
        worker_memory_gib: form.workerMemoryGib,
        worker_disk_gib: form.workerDiskGib,
        pod_cidr: form.podCidr || undefined,
        service_cidr: form.serviceCidr || undefined,
        lb_pool_cidr: form.lbPoolCidr || undefined,
        monitoring: form.monitoring,
      })
      navigate('/containers/kubernetes-clusters')
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Failed to create Kubernetes cluster')
    }
  }

  if (!current) {
    return (
      <div className="az-page">
        <p className="az-text-secondary">
          Select or create a project from the top bar before creating a Kubernetes cluster.
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
            { label: 'Kubernetes clusters', to: '/containers/kubernetes-clusters' },
            { label: 'Create' },
          ]}
        />
        <h1>Create a Kubernetes cluster</h1>
        <p className="az-text-secondary">
          Ships as a single-control-plane K3s cluster with Cilium (LB-IPAM) and Longhorn
          preinstalled. HA control planes aren't supported yet.
        </p>
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
                    ? 'This host has no adopted nodes yet — configure one from its Nodes tab first.'
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
                label="Image"
                value={form.image}
                onChange={(e) => setForm({ ...form, image: e.target.value })}
                required
                hint="Proxmox template VMID — used for both the control plane and workers"
              />
              <TextField
                label="K3s version"
                value={form.k3sVersion}
                onChange={(e) => setForm({ ...form, k3sVersion: e.target.value })}
                hint="Leave blank to install K3s's current stable release"
              />
              <div>
                <Button type="button" variant="primary" onClick={() => setTab('sizing')}>
                  Next: Cluster sizing
                </Button>
              </div>
            </div>
          )}

          {tab === 'sizing' && (
            <div className="az-stack-col az-gap-4" style={{ maxWidth: 480 }}>
              <h3>Control plane</h3>
              <TextField
                label="CPU"
                type="number"
                min={1}
                value={form.controlPlaneCpu}
                onChange={(e) => setForm({ ...form, controlPlaneCpu: Number(e.target.value) })}
                required
              />
              <TextField
                label="Memory (GiB)"
                type="number"
                min={1}
                value={form.controlPlaneMemoryGib}
                onChange={(e) =>
                  setForm({ ...form, controlPlaneMemoryGib: Number(e.target.value) })
                }
                required
              />
              <TextField
                label="Disk (GiB)"
                type="number"
                min={1}
                value={form.controlPlaneDiskGib}
                onChange={(e) => setForm({ ...form, controlPlaneDiskGib: Number(e.target.value) })}
                required
              />

              <h3>Workers</h3>
              <TextField
                label="Worker count"
                type="number"
                min={0}
                value={form.workerCount}
                onChange={(e) => setForm({ ...form, workerCount: Number(e.target.value) })}
                required
                hint="0 gives a single-node cluster (control plane only)"
              />
              <TextField
                label="CPU per worker"
                type="number"
                min={1}
                value={form.workerCpu}
                onChange={(e) => setForm({ ...form, workerCpu: Number(e.target.value) })}
                required
              />
              <TextField
                label="Memory per worker (GiB)"
                type="number"
                min={1}
                value={form.workerMemoryGib}
                onChange={(e) => setForm({ ...form, workerMemoryGib: Number(e.target.value) })}
                required
              />
              <TextField
                label="Disk per worker (GiB)"
                type="number"
                min={1}
                value={form.workerDiskGib}
                onChange={(e) => setForm({ ...form, workerDiskGib: Number(e.target.value) })}
                required
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
                label="IP pool"
                value={form.ipPool}
                onChange={(e) => setForm({ ...form, ipPool: e.target.value })}
                required
                hint="The control plane needs a known static address up front — required, unlike a plain VM's optional pool."
              >
                <option value="" disabled>
                  Select an IP pool
                </option>
                {ipPools.data?.map((pool) => (
                  <option key={pool.name} value={pool.name}>
                    {pool.name} ({pool.cidr}, bridge {pool.bridge})
                  </option>
                ))}
              </Select>
              <TextField
                label="Pod CIDR"
                value={form.podCidr}
                onChange={(e) => setForm({ ...form, podCidr: e.target.value })}
              />
              <TextField
                label="Service CIDR"
                value={form.serviceCidr}
                onChange={(e) => setForm({ ...form, serviceCidr: e.target.value })}
              />
              <TextField
                label="LoadBalancer pool CIDR (optional)"
                value={form.lbPoolCidr}
                onChange={(e) => setForm({ ...form, lbPoolCidr: e.target.value })}
                hint="Cilium LB-IPAM range for LoadBalancer services, e.g. 10.0.202.200/29 (L2 mode only). Leave blank to skip installing an IP pool."
              />
              <label className="az-stack-row az-gap-2">
                <input
                  type="checkbox"
                  checked={form.monitoring}
                  onChange={(e) => setForm({ ...form, monitoring: e.target.checked })}
                />
                Install monitoring (kube-prometheus-stack)
              </label>
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
                    <strong>Image:</strong> {form.image || '—'}
                  </div>
                  <div>
                    <strong>K3s version:</strong> {form.k3sVersion || 'Latest stable'}
                  </div>
                  <div>
                    <strong>Control plane:</strong> {form.controlPlaneCpu} vCPU,{' '}
                    {form.controlPlaneMemoryGib} GiB RAM, {form.controlPlaneDiskGib} GiB disk
                  </div>
                  <div>
                    <strong>Workers:</strong> {form.workerCount} × ({form.workerCpu} vCPU,{' '}
                    {form.workerMemoryGib} GiB RAM, {form.workerDiskGib} GiB disk)
                  </div>
                  <div>
                    <strong>IP pool:</strong> {form.ipPool || '—'}
                  </div>
                  <div>
                    <strong>Bridge:</strong>{' '}
                    {selectedPool ? `${selectedPool.bridge} (from pool)` : '—'}
                  </div>
                  <div>
                    <strong>Pod / Service CIDR:</strong> {form.podCidr} / {form.serviceCidr}
                  </div>
                  <div>
                    <strong>LoadBalancer pool:</strong> {form.lbPoolCidr || 'Not installed'}
                  </div>
                  <div>
                    <strong>Monitoring:</strong> {form.monitoring ? 'Installed' : 'Not installed'}
                  </div>
                </dl>
              </div>
              {error && <p className="az-alert az-alert-danger">{error}</p>}
              <div className="az-stack-row az-gap-2">
                <Button
                  type="submit"
                  variant="primary"
                  disabled={
                    createCluster.isPending ||
                    !form.name ||
                    !form.providerId ||
                    !form.node ||
                    !form.image ||
                    !form.ipPool
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
