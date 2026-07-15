import { useState } from 'react'
import type { FormEvent } from 'react'
import { useCurrentProject } from '../../hooks/useCurrentProject'
import { type ProjectRow, useCreateProject, useDeleteProject, useProjects } from '../../api/projects'
import { ApiError } from '../../api/client'
import { Button } from '../../ui/Button'
import { CommandBar } from '../../ui/CommandBar'
import { DataTable, type DataTableColumn } from '../../ui/DataTable'
import { Modal } from '../../ui/Modal'
import { TextField } from '../../ui/TextField'

export function ProjectsPage() {
  const projects = useProjects()
  const createProject = useCreateProject()
  const deleteProject = useDeleteProject()
  const { current, setCurrent } = useCurrentProject()

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
    if (pendingDelete === current) setCurrent(null)
    setPendingDelete(null)
  }

  const columns: DataTableColumn<ProjectRow>[] = [
    {
      key: 'name',
      header: 'Name',
      render: (row) => (
        <button type="button" className="az-table-link" onClick={() => setCurrent(row.name)}>
          {row.name}
          {row.name === current ? ' (current)' : ''}
        </button>
      ),
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
    <div className="az-page">
      <div className="az-stack-col az-gap-4">
        <h1>Projects</h1>
        <p className="az-text-secondary">
          Click a project to make it the current project — resources across every service hub are
          scoped to whichever project is selected.
        </p>
        <CommandBar>
          <Button variant="primary" onClick={() => setCreateOpen(true)}>
            + Create
          </Button>
        </CommandBar>

        {projects.isLoading && <p>Loading…</p>}
        {projects.isError && <p className="az-alert az-alert-danger">Failed to load projects.</p>}
        {projects.data && projects.data.length === 0 && <p>No projects yet.</p>}
        {projects.data && projects.data.length > 0 && (
          <DataTable columns={columns} data={projects.data} keyField="id" />
        )}
      </div>

      <Modal open={createOpen} title="Create a project" onClose={() => setCreateOpen(false)}>
        <form onSubmit={handleCreate}>
          <div className="az-stack-col az-gap-4">
            <TextField label="Name" value={name} onChange={(e) => setName(e.target.value)} required autoFocus />
            {error && <p className="az-alert az-alert-danger">{error}</p>}
            <Button type="submit" variant="primary" disabled={createProject.isPending}>
              Create
            </Button>
          </div>
        </form>
      </Modal>

      <Modal open={pendingDelete !== null} title="Delete project" onClose={() => setPendingDelete(null)}>
        <div className="az-stack-col az-gap-4">
          <p>
            Delete project <strong>{pendingDelete}</strong>? This cannot be undone.
          </p>
          <div className="az-stack-row az-gap-2">
            <Button variant="primary" onClick={handleDelete} disabled={deleteProject.isPending}>
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
