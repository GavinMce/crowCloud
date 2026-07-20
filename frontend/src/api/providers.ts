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
  /** No longer set at host-creation time — nodes are adopted individually
   * via the host's Nodes tab. May still be present on hosts created before
   * that existed. */
  node?: string
  default_storage?: string
  default_bridge?: string
  tls_insecure?: boolean
  /** Required for VMs with custom cloud-init (K8sCluster's bootstrap
   * scripts, or any VM created with cloud-init user_data) — Proxmox's REST
   * API has no upload endpoint for cloud-init "snippets", so crowCloud
   * SSHes in to place them directly. Plain VM creation works without this. */
  ssh_user?: string
  ssh_port?: number
  /** Comes back masked ("••••••••") once set — never the real value. */
  ssh_private_key?: string
  /** Injected into every VM's `authorized_keys` before anything else in
   * its cloud-init script runs, so a bootstrap failure is still debuggable
   * afterward. Not sensitive — a public key, returned unmasked. Doesn't
   * need to be the pair of `ssh_private_key` above (that one's for
   * crowCloud to reach the host; this one's for a human to reach a VM). */
  ssh_public_key?: string
  /** `false` disables hardware-accelerated virtualization (Proxmox's
   * `kvm=0`) — only needed when the host itself has no VT-x/AMD-V
   * available (e.g. a nested/virtualized Proxmox install). VMs run much
   * slower with this off. Defaults to `true`. */
  kvm?: boolean
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
