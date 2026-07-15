import type { InputHTMLAttributes } from 'react'
import { useId } from 'react'

interface TextFieldProps extends InputHTMLAttributes<HTMLInputElement> {
  label: string
  hint?: string
}

export function TextField({ label, hint, id, className, ...rest }: TextFieldProps) {
  const generatedId = useId()
  const inputId = id ?? generatedId
  return (
    <div className="az-field">
      <label className="az-field-label" htmlFor={inputId}>
        {label}
      </label>
      <input id={inputId} className={`az-field-input ${className ?? ''}`.trim()} {...rest} />
      {hint && <span className="az-field-hint">{hint}</span>}
    </div>
  )
}
