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

/**
 * 前後移動的按鈕（‹ › 這類）按下時，圖示往該方向彈一下再回位。
 * 用法：use:bounce={-1} 往左、use:bounce={1} 往右；滑鼠與鍵盤觸發的 click 都會播。
 */
export function bounce(node: HTMLElement, direction: -1 | 1) {
  let dir = direction;
  const name = () => (dir < 0 ? 'is-bouncing-left' : 'is-bouncing-right');

  function play() {
    if (reducedMotion()) return;
    node.classList.remove('is-bouncing-left', 'is-bouncing-right');
    // 強制重排：連點時動畫從頭再播。
    void node.offsetWidth;
    node.classList.add(name());
  }

  function done(event: AnimationEvent) {
    if (event.animationName.startsWith('icon-bounce')) node.classList.remove('is-bouncing-left', 'is-bouncing-right');
  }

  node.addEventListener('click', play);
  node.addEventListener('animationend', done);
  return {
    update(next: -1 | 1) {
      dir = next;
    },
    destroy() {
      node.removeEventListener('click', play);
      node.removeEventListener('animationend', done);
    },
  };
}
