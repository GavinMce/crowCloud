import { apiFetch, setToken } from './client'

export interface Claims {
  sub: string
  username: string
  is_admin: boolean
  exp: number
  iat: number
}

export function decodeToken(token: string): Claims | null {
  try {
    const payload = token.split('.')[1]
    if (!payload) return null
    const base64 = payload.replace(/-/g, '+').replace(/_/g, '/')
    const json = decodeURIComponent(
      atob(base64)
        .split('')
        .map((c) => '%' + c.charCodeAt(0).toString(16).padStart(2, '0'))
        .join(''),
    )
    return JSON.parse(json) as Claims
  } catch {
    return null
  }
}

interface LoginResponse {
  token: string
}

export async function login(username: string, password: string): Promise<void> {
  const { token } = await apiFetch<LoginResponse>('/auth/login', {
    method: 'POST',
    body: JSON.stringify({ username, password }),
  })
  setToken(token)
}

export interface RegisterResponse {
  id: string
  username: string
  email: string
}

export async function register(
  username: string,
  email: string,
  password: string,
): Promise<RegisterResponse> {
  return apiFetch<RegisterResponse>('/auth/register', {
    method: 'POST',
    body: JSON.stringify({ username, email, password }),
  })
}
