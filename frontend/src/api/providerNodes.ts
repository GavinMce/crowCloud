import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiFetch } from './client'

export type ProviderNode = {
  name: string
  status: string
  cpu: number | null
  max_cpu: number | null
  mem: number | null
  max_mem: number | null
  uptime: number | null
  configured: boolean
  default_storage: string | null
  default_bridge: string | null
}

export interface ConfigureNodeRequest {
  default_storage: string
  default_bridge: string
}

function nodesKey(providerId: string) {
  return ['providers', providerId, 'nodes']
}

export function useProviderNodes(providerId: string | null) {
  return useQuery({
    queryKey: providerId ? nodesKey(providerId) : ['providers', null, 'nodes'],
    queryFn: () => apiFetch<ProviderNode[]>(`/providers/${encodeURIComponent(providerId!)}/nodes`),
    enabled: providerId !== null,
  })
}

export function useProviderNode(providerId: string | null, name: string | null) {
  return useQuery({
    queryKey: providerId && name ? [...nodesKey(providerId), name] : ['providers', null, 'nodes', null],
    queryFn: () =>
      apiFetch<ProviderNode>(
        `/providers/${encodeURIComponent(providerId!)}/nodes/${encodeURIComponent(name!)}`,
      ),
    enabled: providerId !== null && name !== null,
  })
}

export function useConfigureProviderNode(providerId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ name, ...req }: ConfigureNodeRequest & { name: string }) =>
      apiFetch<ProviderNode>(
        `/providers/${encodeURIComponent(providerId)}/nodes/${encodeURIComponent(name)}`,
        { method: 'PUT', body: JSON.stringify(req) },
      ),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: nodesKey(providerId) })
    },
  })
}
