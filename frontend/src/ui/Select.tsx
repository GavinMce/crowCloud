import type { SelectHTMLAttributes } from 'react'
import { useId } from 'react'

interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  label: string
  hint?: string
}

export function Select({ label, hint, id, className, children, ...rest }: SelectProps) {
  const generatedId = useId()
  const selectId = id ?? generatedId
  return (
    <div className="az-field">
      <label className="az-field-label" htmlFor={selectId}>
        {label}
      </label>
      <select id={selectId} className={`az-field-select ${className ?? ''}`.trim()} {...rest}>
        {children}
      </select>
      {hint && <span className="az-field-hint">{hint}</span>}
    </div>
  )
}
