import { useState } from 'react'
import type { FormEvent } from 'react'
import { Badge, Button, Container, Input, Modal, Stack, Table, Tabs, type TableColumn } from '@crow-dev/ui'
import { useParams } from 'react-router-dom'
import { useProjects } from '../api/projects'
import { useProviders } from '../api/providers'
import {
  type CreateVmRequest,
  type ResourceRow,
  useCreateVm,
  useDeleteResource,
  useResource,
  useResources,
} from '../api/resources'
import { ApiError } from '../api/client'

const PHASE_VARIANT: Record<string, 'success' | 'warning' | 'danger' | 'default'> = {
  Ready: 'success',
  Pending: 'warning',
  Failed: 'danger',
}

export function ProjectDetailPage() {
  const { projectName = '' } = useParams()
  const [tab, setTab] = useState('overview')

  const projects = useProjects()
  const project = projects.data?.find((p) => p.name === projectName)

  return (
    <Container maxWidth="lg">
      <Stack direction="column" gap={4}>
        <h1>{projectName}</h1>
        <Tabs
          tabs={[
            { id: 'overview', label: 'Overview' },
            { id: 'resources', label: 'Resources' },
          ]}
          activeTab={tab}
          onChange={setTab}
        />
        {tab === 'overview' && (
          <Stack direction="column" gap={2}>
            <p>
              <strong>Name:</strong> {projectName}
            </p>
            {project && (
              <p>
                <strong>Created:</strong> {new Date(project.created_at).toLocaleString()}
              </p>
            )}
          </Stack>
        )}
        {tab === 'resources' && <ResourcesTab project={projectName} />}
      </Stack>
    </Container>
  )
}

function ResourcesTab({ project }: { project: string }) {
  const resources = useResources(project)
  const providers = useProviders()
  const createVm = useCreateVm(project)
  const deleteResource = useDeleteResource(project)

  const [createOpen, setCreateOpen] = useState(false)
  const [viewName, setViewName] = useState<string | null>(null)
  const [pendingDelete, setPendingDelete] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [form, setForm] = useState({
    name: '',
    providerId: '',
    cpu: 2,
    memoryMib: 2048,
    diskGib: 20,
    image: '',
  })

  const detail = useResource(project, viewName)

  const handleCreate = async (e: FormEvent) => {
    e.preventDefault()
    setError(null)
    const req: CreateVmRequest = {
      name: form.name,
      provider_id: form.providerId,
      cpu: form.cpu,
      memory_mib: form.memoryMib,
      disk_gib: form.diskGib,
      image: form.image,
    }
    try {
      await createVm.mutateAsync(req)
      setCreateOpen(false)
      setForm({ name: '', providerId: '', cpu: 2, memoryMib: 2048, diskGib: 20, image: '' })
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Failed to create VM')
    }
  }

  const handleDelete = async () => {
    if (!pendingDelete) return
    await deleteResource.mutateAsync(pendingDelete)
    setPendingDelete(null)
  }

  const columns: TableColumn<ResourceRow>[] = [
    {
      key: 'name',
      header: 'Name',
      render: (row) => (
        <button type="button" onClick={() => setViewName(row.name)}>
          {row.name}
        </button>
      ),
    },
    { key: 'resource_type', header: 'Type' },
    {
      key: 'phase',
      header: 'Phase',
      render: (row) => <Badge variant={PHASE_VARIANT[row.phase] ?? 'default'}>{row.phase}</Badge>,
    },
    {
      key: 'created_at',
      header: 'Created',
      render: (row) => new Date(row.created_at).toLocaleString(),
    },
    {
      key: 'id',
      header: '',
      render: (row) => (
        <Button variant="ghost" size="sm" onClick={() => setPendingDelete(row.name)}>
          Delete
        </Button>
      ),
    },
  ]

  return (
    <Stack direction="column" gap={4}>
      <Stack direction="row" justify="between" align="center">
        <h2>Resources</h2>
        <Button variant="primary" onClick={() => setCreateOpen(true)}>
          New VM
        </Button>
      </Stack>

      {resources.isLoading && <p>Loading…</p>}
      {resources.isError && <p role="alert">Failed to load resources.</p>}
      {resources.data && resources.data.length === 0 && <p>No resources yet.</p>}
      {resources.data && resources.data.length > 0 && (
        <Table columns={columns} data={resources.data} keyField="id" />
      )}

      <Modal open={createOpen} onClose={() => setCreateOpen(false)} title="New VM">
        <form onSubmit={handleCreate}>
          <Stack direction="column" gap={4}>
            <Input
              label="Name"
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
              required
              autoFocus
            />
            <label>
              Provider
              <select
                value={form.providerId}
                onChange={(e) => setForm({ ...form, providerId: e.target.value })}
                required
              >
                <option value="" disabled>
                  Select a provider
                </option>
                {providers.data?.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            </label>
            <Input
              label="CPU"
              type="number"
              min={1}
              value={form.cpu}
              onChange={(e) => setForm({ ...form, cpu: Number(e.target.value) })}
              required
            />
            <Input
              label="Memory (MiB)"
              type="number"
              min={1024}
              step={1024}
              value={form.memoryMib}
              onChange={(e) => setForm({ ...form, memoryMib: Number(e.target.value) })}
              required
            />
            <Input
              label="Disk (GiB)"
              type="number"
              min={1}
              value={form.diskGib}
              onChange={(e) => setForm({ ...form, diskGib: Number(e.target.value) })}
              required
            />
            <Input
              label="Image"
              value={form.image}
              onChange={(e) => setForm({ ...form, image: e.target.value })}
              required
            />
            {error && <p role="alert">{error}</p>}
            <Button type="submit" variant="primary" disabled={createVm.isPending}>
              Create
            </Button>
          </Stack>
        </form>
      </Modal>

      <Modal open={viewName !== null} onClose={() => setViewName(null)} title={viewName ?? ''}>
        {detail.isLoading && <p>Loading…</p>}
        {detail.data && (
          <Stack direction="column" gap={2}>
            <p>
              <strong>Phase:</strong> {detail.data.phase}
            </p>
            <p>
              <strong>Created:</strong> {new Date(detail.data.created_at).toLocaleString()}
            </p>
            <pre>{JSON.stringify(detail.data.handle, null, 2)}</pre>
          </Stack>
        )}
      </Modal>

      <Modal
        open={pendingDelete !== null}
        onClose={() => setPendingDelete(null)}
        title="Delete resource"
      >
        <Stack direction="column" gap={4}>
          <p>
            Delete resource <strong>{pendingDelete}</strong>? This cannot be undone.
          </p>
          <Stack direction="row" gap={2}>
            <Button variant="primary" onClick={handleDelete} disabled={deleteResource.isPending}>
              Delete
            </Button>
            <Button variant="secondary" onClick={() => setPendingDelete(null)}>
              Cancel
            </Button>
          </Stack>
        </Stack>
      </Modal>
    </Stack>
  )
}
