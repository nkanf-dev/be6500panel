/** Trim unchanged edges; deterministic, bounded, and does not persist content. */
export function nativeConfigDiff(previous: string, candidate: string): string {
  if (previous === candidate) return "无内容变化";
  const before = previous.split("\n");
  const after = candidate.split("\n");
  let head = 0;
  let tail = 0;
  while (
    head < before.length &&
    head < after.length &&
    before[head] === after[head]
  )
    head++;
  while (
    tail < before.length - head &&
    tail < after.length - head &&
    before[before.length - tail - 1] === after[after.length - tail - 1]
  )
    tail++;
  const removed = before.slice(head, before.length - tail);
  const added = after.slice(head, after.length - tail);
  return [
    `@@ 第 ${head + 1} 行 · -${removed.length} / +${added.length} @@`,
    ...removed.map((line) => `- ${line}`),
    ...added.map((line) => `+ ${line}`),
  ].join("\n");
}
