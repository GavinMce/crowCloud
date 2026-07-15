import { useState } from 'react'
import type { FormEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import { login, register } from '../api/auth'
import { ApiError } from '../api/client'
import { Button } from '../ui/Button'
import { TextField } from '../ui/TextField'

export function LoginPage() {
  const navigate = useNavigate()
  const [mode, setMode] = useState<'login' | 'register'>('login')
  const [username, setUsername] = useState('')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)

  const handleLogin = async (e: FormEvent) => {
    e.preventDefault()
    setError(null)
    setSubmitting(true)
    try {
      await login(username, password)
      navigate('/', { replace: true })
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Login failed')
    } finally {
      setSubmitting(false)
    }
  }

  const handleRegister = async (e: FormEvent) => {
    e.preventDefault()
    setError(null)
    setSubmitting(true)
    try {
      const res = await register(username, email, password)
      setMode('login')
      setUsername(res.username)
      setPassword('')
      setNotice('Admin account created — log in below.')
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Registration failed')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div
      className="az-stack-col az-gap-6"
      style={{
        alignItems: 'center',
        paddingTop: '10vh',
        minHeight: '100%',
        background: 'var(--az-bg-canvas)',
      }}
    >
      <h1>crowCloud</h1>
      <div className="az-card" style={{ width: 360 }}>
        <h2>{mode === 'login' ? 'Log in' : 'First-time setup'}</h2>
        <form onSubmit={mode === 'login' ? handleLogin : handleRegister}>
          <div className="az-stack-col az-gap-4">
            <TextField
              label="Username"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              required
              autoFocus
            />
            {mode === 'register' && (
              <TextField
                label="Email"
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                required
              />
            )}
            <TextField
              label="Password"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              required
            />
            {error && <p className="az-alert az-alert-danger">{error}</p>}
            {notice && <p>{notice}</p>}
            <Button type="submit" variant="primary" disabled={submitting}>
              {mode === 'login' ? 'Log in' : 'Create admin account'}
            </Button>
          </div>
        </form>
      </div>
      <button
        type="button"
        className="az-table-link"
        onClick={() => {
          setError(null)
          setNotice(null)
          setMode(mode === 'login' ? 'register' : 'login')
        }}
      >
        {mode === 'login'
          ? 'First time setting up crowCloud? Create the admin account'
          : 'Already have an account? Log in'}
      </button>
    </div>
  )
}
