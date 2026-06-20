import styles from './EmptyState.module.scss'

interface EmptyStateProps {
  title: string
  description?: string
}

export function EmptyState({ title, description }: EmptyStateProps) {
  return (
    <div className={styles.empty}>
      <strong>{title}</strong>
      {description ? <span>{description}</span> : null}
    </div>
  )
}
