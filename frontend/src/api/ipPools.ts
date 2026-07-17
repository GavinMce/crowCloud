import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiFetch } from './client'

export type IpPoolRow = {
  name: string
  cidr: string
  range_start: string
  range_end: string
  bridge: string
  allocated: number | null
  available: number | null
}

export interface IpPoolDetail {
  name: string
  cidr: string
  range_start: string
  range_end: string
  gateway: string
  dns: string[]
  bridge: string
  allocated: number | null
  available: number | null
}

export interface CreateIpPoolRequest {
  name: string
  cidr: string
  range_start: string
  range_end: string
  gateway: string
  dns: string[]
  bridge: string
}

const ipPoolsKey = ['ip-pools']

export function useIpPools() {
  return useQuery({
    queryKey: ipPoolsKey,
    queryFn: () => apiFetch<IpPoolRow[]>('/ip-pools'),
  })
}

export function useIpPool(name: string | null) {
  return useQuery({
    queryKey: [...ipPoolsKey, name],
    queryFn: () => apiFetch<IpPoolDetail>(`/ip-pools/${encodeURIComponent(name!)}`),
    enabled: name !== null,
  })
}

export function useCreateIpPool() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (req: CreateIpPoolRequest) =>
      apiFetch<IpPoolDetail>('/ip-pools', {
        method: 'POST',
        body: JSON.stringify(req),
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ipPoolsKey })
    },
  })
}

export function useDeleteIpPool() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (name: string) =>
      apiFetch<void>(`/ip-pools/${encodeURIComponent(name)}`, { method: 'DELETE' }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ipPoolsKey })
    },
  })
}
