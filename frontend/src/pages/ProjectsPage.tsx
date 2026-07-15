import { useState } from 'react'
import type { FormEvent } from 'react'
import { Button, Container, Input, Modal, Stack, Table, type TableColumn } from '@crow-dev/ui'
import { Link } from 'react-router-dom'
import { type ProjectRow, useCreateProject, useDeleteProject, useProjects } from '../api/projects'
import { ApiError } from '../api/client'

export function ProjectsPage() {
  const projects = useProjects()
  const createProject = useCreateProject()
  const deleteProject = useDeleteProject()

  const [createOpen, setCreateOpen] = useState(false)
  const [name, setName] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [pendingDelete, setPendingDelete] = useState<string | null>(null)

  const handleCreate = async (e: FormEvent) => {
    e.preventDefault()
    setError(null)
    try {
      await createProject.mutateAsync(name)
      setName('')
      setCreateOpen(false)
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Failed to create project')
    }
  }

  const handleDelete = async () => {
    if (!pendingDelete) return
    await deleteProject.mutateAsync(pendingDelete)
    setPendingDelete(null)
  }

  const columns: TableColumn<ProjectRow>[] = [
    {
      key: 'name',
      header: 'Name',
      render: (row) => <Link to={`/projects/${encodeURIComponent(row.name)}`}>{row.name}</Link>,
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
    <Container maxWidth="lg">
      <Stack direction="column" gap={4}>
        <Stack direction="row" justify="between" align="center">
          <h1>Projects</h1>
          <Button variant="primary" onClick={() => setCreateOpen(true)}>
            New Project
          </Button>
        </Stack>

        {projects.isLoading && <p>Loading…</p>}
        {projects.isError && <p role="alert">Failed to load projects.</p>}
        {projects.data && projects.data.length === 0 && <p>No projects yet.</p>}
        {projects.data && projects.data.length > 0 && (
          <Table columns={columns} data={projects.data} keyField="id" />
        )}
      </Stack>

      <Modal open={createOpen} onClose={() => setCreateOpen(false)} title="New Project">
        <form onSubmit={handleCreate}>
          <Stack direction="column" gap={4}>
            <Input
              label="Name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
              autoFocus
            />
            {error && <p role="alert">{error}</p>}
            <Button type="submit" variant="primary" disabled={createProject.isPending}>
              Create
            </Button>
          </Stack>
        </form>
      </Modal>

      <Modal
        open={pendingDelete !== null}
        onClose={() => setPendingDelete(null)}
        title="Delete project"
      >
        <Stack direction="column" gap={4}>
          <p>
            Delete project <strong>{pendingDelete}</strong>? This cannot be undone.
          </p>
          <Stack direction="row" gap={2}>
            <Button variant="primary" onClick={handleDelete} disabled={deleteProject.isPending}>
              Delete
            </Button>
            <Button variant="secondary" onClick={() => setPendingDelete(null)}>
              Cancel
            </Button>
          </Stack>
        </Stack>
      </Modal>
    </Container>
  )
}
