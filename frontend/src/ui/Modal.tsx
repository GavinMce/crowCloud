import type { ReactNode } from 'react'
import { CloseIcon } from './icons'

interface ModalProps {
  open: boolean
  title: string
  onClose: () => void
  children: ReactNode
}

export function Modal({ open, title, onClose, children }: ModalProps) {
  if (!open) return null
  return (
    <div className="az-modal-overlay" onClick={onClose}>
      <div className="az-modal" onClick={(e) => e.stopPropagation()}>
        <div className="az-modal-header">
          <h2 className="az-modal-title">{title}</h2>
          <button
            type="button"
            className="az-modal-close"
            onClick={onClose}
            aria-label="Close dialog"
          >
            <CloseIcon size={16} />
          </button>
        </div>
        <div className="az-modal-body">{children}</div>
      </div>
    </div>
  )
}
