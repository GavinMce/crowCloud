import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'

interface ServiceTileProps {
  icon: ReactNode
  title: string
  description: string
  to: string
}

export function ServiceTile({ icon, title, description, to }: ServiceTileProps) {
  return (
    <Link to={to} className="az-tile">
      <span className="az-tile-icon">{icon}</span>
      <span>
        <p className="az-tile-title">{title}</p>
        <p className="az-tile-desc">{description}</p>
      </span>
    </Link>
  )
}
