/**
 * 設定頁清除 Wiki 快取時遞增；常駐的 Wiki 搜尋分頁據此丟掉手上的索引狀態，
 * 下次打開會重新向 Rust 載入。
 */
export const cacheEvents = $state({ wikiGeneration: 0 });
