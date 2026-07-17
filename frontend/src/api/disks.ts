import { useMutation, useQueryClient } from '@tanstack/react-query'
import { apiFetch } from './client'
import type { ResourceRow } from './resources'

export interface DiskHandleData {
  size_gib: number
  attached_size_gib: number | null
  volid: string | null
  attached_vm_ref: { name: string; namespace: string | null } | null
}

export interface CreateDiskRequest {
  name: string
  provider_id: string
  node: string
  size_gib: number
  /** Attach immediately — a VM's `resources` name in the same project. */
  vm_name?: string
}

export interface UpdateDiskRequest {
  detach?: boolean
  vm_name?: string
  size_gib?: number
}

/** Narrows a generic resource's `handle` into disk-specific data — the
 * operator writes `{resource_type:"Disk", data: DiskHandleData}` there. */
export function parseDiskHandle(handle: unknown): DiskHandleData | null {
  if (!handle || typeof handle !== 'object' || !('data' in handle)) return null
  const data = (handle as { data?: unknown }).data
  if (!data || typeof data !== 'object' || !('size_gib' in data)) return null
  return data as DiskHandleData
}

export function useCreateDisk(project: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (req: CreateDiskRequest) =>
      apiFetch<ResourceRow>(`/projects/${encodeURIComponent(project)}/resources`, {
        method: 'POST',
        body: JSON.stringify({ resource_type: 'disk', ...req }),
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['resources', project] })
    },
  })
}

export function useUpdateDisk(project: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ name, req }: { name: string; req: UpdateDiskRequest }) =>
      apiFetch<ResourceRow>(
        `/projects/${encodeURIComponent(project)}/resources/${encodeURIComponent(name)}`,
        {
          method: 'PATCH',
          body: JSON.stringify(req),
        },
      ),
    onSuccess: (_data, { name }) => {
      void queryClient.invalidateQueries({ queryKey: ['resources', project] })
      void queryClient.invalidateQueries({ queryKey: ['resources', project, name] })
    },
  })
}
