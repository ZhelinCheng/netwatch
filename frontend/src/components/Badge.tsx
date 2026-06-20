import type { ReactNode } from 'react'
import styles from './Badge.module.scss'

type Tone = 'blue' | 'green' | 'red' | 'gray' | 'orange' | 'purple' | 'cyan'

interface BadgeProps {
  children: ReactNode
  tone?: Tone
}

export function Badge({ children, tone = 'gray' }: BadgeProps) {
  return <span className={`${styles.badge} ${styles[tone]}`}>{children}</span>
}
