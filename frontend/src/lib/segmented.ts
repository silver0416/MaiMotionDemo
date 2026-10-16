import { reducedMotion } from './press';

/** 分段控制的選中項目：aria-pressed／aria-selected／aria-checked 或 .is-active。 */
const ACTIVE =
  ':scope > [aria-pressed="true"], :scope > [aria-selected="true"], :scope > [aria-checked="true"], :scope > .is-active';

/** 膠囊移動的基準時間（Claude Design「互動動畫」2b）。 */
const DURATION = 240;
/** 前緣先到、後緣跟上：移動途中膠囊會略為拉長。 */
const LEAD = 0.7;
const TRAIL = 1.15;

/**
 * 分段控制的滑動膠囊。掛在 .seg 容器上，膠囊用容器的 ::before 畫，
 * 位置寫進 CSS 變數，不在 Svelte 管理的 DOM 裡插入節點。
 *
 * 選中項目改變（屬性或 class 變動）時，膠囊從舊位置滑到新位置；
 * 往右移時右緣先到、左緣跟上，往左則相反。第一次定位不播動畫。
 */
export function segmented(node: HTMLElement) {
  let lastLeft: number | null = null;
  let frame = 0;

  function place() {
    frame = 0;
    const active = node.querySelector<HTMLElement>(ACTIVE);
    if (!active || active.offsetWidth === 0) {
      node.style.setProperty('--seg-o', '0');
      return;
    }
    const left = active.offsetLeft;
    const right = node.clientWidth - (active.offsetLeft + active.offsetWidth);
    const motion = lastLeft !== null && !reducedMotion();
    if (motion && left !== lastLeft) {
      const goingRight = left > (lastLeft ?? 0);
      node.style.setProperty('--seg-ld', `${Math.round(DURATION * (goingRight ? TRAIL : LEAD))}ms`);
      node.style.setProperty('--seg-rd', `${Math.round(DURATION * (goingRight ? LEAD : TRAIL))}ms`);
    } else if (!motion) {
      node.style.setProperty('--seg-ld', '0ms');
      node.style.setProperty('--seg-rd', '0ms');
    }
    node.style.setProperty('--seg-l', `${left}px`);
    node.style.setProperty('--seg-r', `${right}px`);
    node.style.setProperty('--seg-o', '1');
    lastLeft = left;
  }

  function schedule() {
    if (!frame) frame = requestAnimationFrame(place);
  }

  // 尺寸改變（拖曳欄寬、字型載入）只重新定位，不播動畫。
  function relayout() {
    lastLeft = null;
    schedule();
  }

  const mutations = new MutationObserver(schedule);
  mutations.observe(node, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeFilter: ['class', 'aria-pressed', 'aria-selected', 'aria-checked'],
  });
  const resize = new ResizeObserver(relayout);
  resize.observe(node);

  node.classList.add('has-seg');
  place();

  return {
    destroy() {
      cancelAnimationFrame(frame);
      mutations.disconnect();
      resize.disconnect();
    },
  };
}
