import { clearToken, getToken } from '../api/client'
import { decodeToken } from '../api/auth'

export function useAuth() {
  const token = getToken()
  const claims = token ? decodeToken(token) : null
  const isExpired = !claims || claims.exp * 1000 < Date.now()
  const isAuthenticated = claims !== null && !isExpired

  return {
    isAuthenticated,
    isAdmin: isAuthenticated && claims.is_admin,
    username: isAuthenticated ? claims.username : undefined,
    logout: () => {
      clearToken()
      window.location.assign('/login')
    },
  }
}
