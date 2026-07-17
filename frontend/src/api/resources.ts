import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiFetch } from './client'

export type ResourceRow = {
  id: string
  name: string
  resource_type: string
  provider_id: string | null
  phase: string
  created_at: string
}

export interface ResourceDetail {
  id: string
  name: string
  resource_type: string
  phase: string
  handle: unknown
  created_at: string
}

export interface CreateVmRequest {
  name: string
  provider_id: string
  node: string
  cpu: number
  memory_mib: number
  disk_gib: number
  image: string
  ip_pool?: string
  /** Only meaningful when `ip_pool` is set. Matches the backend's `IpMode` enum. */
  ip_mode?: 'Static' | 'Dhcp'
  /** Only meaningful when `ip_pool` is set and `ip_mode` is `Static` (the default). */
  requested_ip?: string
}

function resourcesKey(project: string) {
  return ['resources', project]
}

export function useResources(project: string) {
  return useQuery({
    queryKey: resourcesKey(project),
    queryFn: () => apiFetch<ResourceRow[]>(`/projects/${encodeURIComponent(project)}/resources`),
    enabled: project.length > 0,
  })
}

export function useResource(project: string, name: string | null) {
  return useQuery({
    queryKey: [...resourcesKey(project), name],
    queryFn: () =>
      apiFetch<ResourceDetail>(
        `/projects/${encodeURIComponent(project)}/resources/${encodeURIComponent(name!)}`,
      ),
    enabled: project.length > 0 && name !== null,
  })
}

export function useCreateVm(project: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (req: CreateVmRequest) =>
      apiFetch<ResourceRow>(`/projects/${encodeURIComponent(project)}/resources`, {
        method: 'POST',
        body: JSON.stringify({ resource_type: 'vm', ...req }),
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: resourcesKey(project) })
    },
  })
}

export function useDeleteResource(project: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (name: string) =>
      apiFetch<void>(
        `/projects/${encodeURIComponent(project)}/resources/${encodeURIComponent(name)}`,
        { method: 'DELETE' },
      ),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: resourcesKey(project) })
    },
  })
}
