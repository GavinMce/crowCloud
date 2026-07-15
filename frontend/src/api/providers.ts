import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiFetch } from './client'

export type ProviderRow = {
  id: string
  name: string
  provider_type: string
  created_at: string
}

export interface ProxmoxConfig {
  url: string
  token_id: string
  token_secret: string
  node: string
  default_storage: string
  default_bridge: string
  tls_insecure?: boolean
}

export interface CreateProviderRequest {
  name: string
  provider_type: string
  config: ProxmoxConfig
}

/** `config.token_secret` comes back masked ("••••••••") — never the real value. */
export interface ProviderDetail {
  id: string
  name: string
  provider_type: string
  config: ProxmoxConfig
  created_at: string
}

export function useProviders() {
  return useQuery({
    queryKey: ['providers'],
    queryFn: () => apiFetch<ProviderRow[]>('/providers'),
  })
}

export function useProvider(id: string | null) {
  return useQuery({
    queryKey: ['providers', id],
    queryFn: () => apiFetch<ProviderDetail>(`/providers/${encodeURIComponent(id!)}`),
    enabled: id !== null,
  })
}

export function useCreateProvider() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (req: CreateProviderRequest) =>
      apiFetch<ProviderRow>('/providers', {
        method: 'POST',
        body: JSON.stringify(req),
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['providers'] })
    },
  })
}

export function useUpdateProvider() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, config }: { id: string; config: Partial<ProxmoxConfig> }) =>
      apiFetch<ProviderDetail>(`/providers/${encodeURIComponent(id)}`, {
        method: 'PATCH',
        body: JSON.stringify({ config }),
      }),
    onSuccess: (_data, { id }) => {
      void queryClient.invalidateQueries({ queryKey: ['providers'] })
      void queryClient.invalidateQueries({ queryKey: ['providers', id] })
    },
  })
}

export function useDeleteProvider() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) =>
      apiFetch<void>(`/providers/${encodeURIComponent(id)}`, { method: 'DELETE' }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['providers'] })
    },
  })
}
