const encoder = typeof TextEncoder === 'undefined' ? null : new TextEncoder();

/**
 * 契約的 sourceSpan 是原文 UTF-8 byte offset，不能直接當成 JS 字串（UTF-16）索引。
 * 這裡逐字累加 UTF-8 長度換算成可用於 textarea 的索引。
 */
export function byteOffsetToIndex(source: string, byteOffset: number): number {
  if (byteOffset <= 0) return 0;
  if (!encoder) return Math.min(byteOffset, source.length);
  let bytes = 0;
  let index = 0;
  for (const char of source) {
    if (bytes >= byteOffset) break;
    bytes += encoder.encode(char).length;
    index += char.length;
  }
  return Math.min(index, source.length);
}

export function lineOf(source: string, line: number): string {
  return source.split('\n')[line - 1] ?? '';
}
