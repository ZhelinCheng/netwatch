import styles from './Switch.module.scss'

interface SwitchProps {
  checked: boolean
  disabled?: boolean
  label: string
  onChange?: (checked: boolean) => void
}

export function Switch({ checked, disabled, label, onChange }: SwitchProps) {
  return (
    <button
      type="button"
      className={`${styles.switch} ${checked ? styles.checked : ''}`}
      aria-label={label}
      aria-pressed={checked}
      disabled={disabled}
      onClick={() => onChange?.(!checked)}
    >
      <span />
    </button>
  )
}
