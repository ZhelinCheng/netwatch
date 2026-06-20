import { ChevronLeft, ChevronRight } from 'lucide-react'
import styles from './Pagination.module.scss'

interface PaginationProps {
  page: number
  pageCount: number
  total: number
  pageSize: number
  onPageChange: (page: number) => void
  onPageSizeChange?: (pageSize: number) => void
}

export function Pagination({
  page,
  pageCount,
  total,
  pageSize,
  onPageChange,
  onPageSizeChange,
}: PaginationProps) {
  const pages = Array.from({ length: Math.min(pageCount, 5) }, (_, index) => index + 1)

  return (
    <div className={styles.pagination}>
      <span>共 {total} 项</span>
      <div className={styles.controls}>
        <button type="button" disabled={page <= 1} onClick={() => onPageChange(page - 1)}>
          <ChevronLeft size={16} />
        </button>
        {pages.map((item) => (
          <button
            type="button"
            key={item}
            className={item === page ? styles.active : ''}
            onClick={() => onPageChange(item)}
          >
            {item}
          </button>
        ))}
        <button type="button" disabled={page >= pageCount} onClick={() => onPageChange(page + 1)}>
          <ChevronRight size={16} />
        </button>
        {onPageSizeChange ? (
          <select value={pageSize} onChange={(event) => onPageSizeChange(Number(event.target.value))}>
            <option value={10}>10 / 页</option>
            <option value={20}>20 / 页</option>
            <option value={50}>50 / 页</option>
          </select>
        ) : null}
      </div>
    </div>
  )
}
