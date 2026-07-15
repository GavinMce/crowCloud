import { useState } from 'react'
import type { FormEvent } from 'react'
import { Button, Card, Container, Input, Stack } from '@crow-dev/ui'
import { useNavigate } from 'react-router-dom'
import { login, register } from '../api/auth'
import { ApiError } from '../api/client'

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
    <Container maxWidth="sm">
      <Stack direction="column" gap={6} align="center" style={{ paddingTop: '4rem' }}>
        <h1>crowCloud</h1>
        <Card header={mode === 'login' ? 'Log in' : 'First-time setup'}>
          <form onSubmit={mode === 'login' ? handleLogin : handleRegister}>
            <Stack direction="column" gap={4}>
              <Input
                label="Username"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                required
                autoFocus
              />
              {mode === 'register' && (
                <Input
                  label="Email"
                  type="email"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  required
                />
              )}
              <Input
                label="Password"
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                required
              />
              {error && <p role="alert">{error}</p>}
              {notice && <p>{notice}</p>}
              <Button type="submit" variant="primary" disabled={submitting}>
                {mode === 'login' ? 'Log in' : 'Create admin account'}
              </Button>
            </Stack>
          </form>
        </Card>
        <button
          type="button"
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
      </Stack>
    </Container>
  )
}
