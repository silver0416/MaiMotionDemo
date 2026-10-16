import { cubicIn } from 'svelte/easing';
import type { TransitionConfig } from 'svelte/transition';
import { reducedMotion } from './press';

/**
 * 浮出面板（下拉選單、右鍵選單、浮動面板）收合：100ms ease-in，整塊一起淡出並往上縮回，不錯開。
 * 開啟的動畫寫在 app.css 的 .pop（淡入 140ms、下移 4px 並由 0.97 放大，180ms）。
 * 用 |global：元件根節點由父層的 {#if} 移除時也要播。
 */
export function popOut(_node: Element): TransitionConfig {
  if (reducedMotion()) return { duration: 0 };
  return {
    duration: 100,
    easing: cubicIn,
    css: (t) => `opacity:${t};transform:translateY(${(t - 1) * 4}px) scale(${0.97 + 0.03 * t})`,
  };
}
