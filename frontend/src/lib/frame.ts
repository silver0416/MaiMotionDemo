/**
 * 下一次畫面繪出之後再執行：先讓按下的回饋（分頁切換、骨架）畫出來，再做重的掛載。
 * 視窗在背景時 requestAnimationFrame 可能不觸發，另設計時器保底，兩者先到者執行一次。
 * 回傳取消函式。
 */
export function afterPaint(run: () => void, fallbackMs = 100): () => void {
  let done = false;
  let inner: ReturnType<typeof setTimeout> | undefined;
  const once = () => {
    if (done) return;
    done = true;
    run();
  };
  const frame = typeof requestAnimationFrame === 'function' ? requestAnimationFrame(() => (inner = setTimeout(once, 0))) : 0;
  const fallback = setTimeout(once, fallbackMs);
  return () => {
    done = true;
    if (frame) cancelAnimationFrame(frame);
    clearTimeout(inner);
    clearTimeout(fallback);
  };
}
