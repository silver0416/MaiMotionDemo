export function reducedMotion(): boolean {
  return typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
}

/**
 * 按下時讓按鈕裡的圖示「壓扁再彈回」。
 * 用 click 觸發，滑鼠與鍵盤（Enter／Space）都有；按鈕隨後被換掉時，動畫仍會在淡出期間播完。
 */
export function squash(node: HTMLElement) {
  function play() {
    if (reducedMotion()) return;
    node.classList.remove('is-squashing');
    // 強制重排，連點時動畫能從頭再播一次。
    void node.offsetWidth;
    node.classList.add('is-squashing');
  }

  function done(event: AnimationEvent) {
    if (event.animationName === 'icon-squash') node.classList.remove('is-squashing');
  }

  node.addEventListener('click', play);
  node.addEventListener('animationend', done);
  return {
    destroy() {
      node.removeEventListener('click', play);
      node.removeEventListener('animationend', done);
    },
  };
}
