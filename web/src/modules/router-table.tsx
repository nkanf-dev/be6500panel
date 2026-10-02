import type { ReactNode } from "react";
import { EmptyState } from "../components/ui/primitives";

export function RouterTable({
  title,
  columns,
  count,
  emptyTitle,
  children,
}: {
  title: string;
  columns: readonly string[];
  count: number;
  emptyTitle: string;
  children: ReactNode;
}) {
  return (
    <div className="table-scroll">
      <table className="data-table" aria-label={title}>
        <thead>
          <tr>
            {columns.map((column) => (
              <th key={column}>{column}</th>
            ))}
          </tr>
        </thead>
        <tbody>{children}</tbody>
      </table>
      {!count && <EmptyState title={emptyTitle} />}
    </div>
  );
}
