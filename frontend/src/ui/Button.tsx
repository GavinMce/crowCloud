import type { ButtonHTMLAttributes } from 'react'

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'default' | 'ghost'
  size?: 'md' | 'sm'
}

export function Button({ variant = 'default', size = 'md', className, ...rest }: ButtonProps) {
  const classes = [
    'az-btn',
    `az-btn-${variant}`,
    size === 'sm' ? 'az-btn-sm' : '',
    className ?? '',
  ]
    .filter(Boolean)
    .join(' ')
  return <button className={classes} type={rest.type ?? 'button'} {...rest} />
}
