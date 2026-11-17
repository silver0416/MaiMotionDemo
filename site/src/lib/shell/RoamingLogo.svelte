<script lang="ts">
  /**
   * 會在盤面上走動的吉祥物：站上的標誌（環＝盤面，粉、藍兩點＝左右手，也是它的眼睛）。
   *
   * 兩點的規則（任何時候都成立，包含移動途中與被打斷時）：
   * - 距離永遠不變：兩點是同一組，整組只做平移＋旋轉。
   * - 不交叉：旋轉角度只在 −90°～90° 之間（藍點不會跑到粉點左邊）。
   * - 只靠近圈、不壓到圈上：每一格畫面都把兩點收在 LIMIT 以內（fit）。
   *
   * 它會做的事：
   * - 走動：第一次捲進畫面時，隨機走四次再回到原位（靠近 1～8 號按鈕、或盤面內任何地方；每次最長 1.5 秒）。不循環。
   * - 躲游標：滑鼠靠近（SENSE 以內）時，兩點往游標的反方向躲，只會躲到八個固定的位置（朝 1～8 號按鈕的方向）；游標離開就回原位。
   * - 往上跑：點一下、或手機上摸一下，兩點直接跑到上面，停一下再回來。
   * - 被搓：按住左右來回搓，眼睛變成「> <」並跟著手指左右移（只有眼睛平移，標誌本身不動）；
   *   放開或停下來之後先維持一下，再睜開眼、移回正中間（不是瞬間跳回去）。
   * - prefers-reduced-motion：不走動、不躲、眼睛不移；被搓時眼睛仍會變。
   *
   * 純裝飾（aria-hidden）。座標用 Logo.svelte 的 viewBox（0～100，圓心 50,50）；位置用 rAF 逐格算，不靠 CSS 轉場，
   * 這樣被打斷（走到一半游標靠過來）時也能從當下的位置接著動，而且每一格都檢查得到。
   */
  import { onMount } from 'svelte';

  interface Props {
    size?: number;
  }

  let { size = 48 }: Props = $props();

  /** 兩點各離中點多遠（Logo 的兩點在 36 與 64，中點 50）。 */
  const HALF = 14;
  /**
   * 點的圓心離盤面中心最遠的距離：圈的內緣在 38 − 10.5 / 2 = 32.75，點的半徑 10，
   * 所以 22.75 會剛好貼到圈；取 22，留一點空隙（靠近、不碰到）。
   */
  const NEAR = 22;
  /** 移動途中的上限：22.75 剛好貼到圈，途中最多到幾乎貼上。 */
  const LIMIT = 22.6;
  /** 相鄰兩顆按鈕的圓心角一半（22.5°）。 */
  const STEP = Math.PI / 8;
  const MOVES = 4;
  const MAX_MS = 1500;
  const MIN_MS = 450;
  /** 游標離中心多近（px）就開始躲。 */
  const SENSE = 120;

  /** 兩點中點相對圓心的位移，以及粉點指向藍點的方向（度，−90～90）。 */
  interface Pose {
    x: number;
    y: number;
    deg: number;
  }

  const HOME: Pose = { x: 0, y: 0, deg: 0 };
  /** 兩點水平時，中點最多能往左右偏多少（兩點都還在 NEAR 以內）。 */
  const SIDE = NEAR - HALF;
  /** 放開或停下來之後，眼睛維持「> <」多久才恢復。 */
  const RUB_HOLD = 700;
  /** 兩點都貼近圈的最上面。 */
  const UP: Pose = { x: 0, y: -Math.sqrt(NEAR * NEAR - HALF * HALF), deg: 0 };

  let pose = $state<Pose>(HOME);
  let rubbing = $state(false);
  let root: HTMLElement;

  let reduced = false;
  let frame = 0;
  let anim: { from: Pose; to: Pose; start: number; ms: number; done?: () => void } | null = null;

  let roaming = false;
  let roamTimer = 0;
  let dodging = false;
  /** 現在躲在哪一個固定位置（0～7，朝第幾顆按鈕的方向）；−1 是沒在躲。 */
  let dodgeSlot = -1;
  /** 往上跑的那一下還沒結束，先不躲。 */
  let holdUntil = 0;
  let hopTimer = 0;

  // 按住時的狀態：pressed、移動量，以及左右換方向的時間（用來判斷是不是在搓）。
  let pressed = false;
  let lastX = 0;
  let dir = 0;
  let stroke = 0;
  let moved = 0;
  let turns: number[] = [];
  let rubTimer = 0;
  /** 按下時標誌中心的位置與每單位幾 px（被搓時眼睛跟著手指用）。 */
  let centerX = 0;
  let unit = 1;

  const rand = (min: number, max: number) => min + Math.random() * (max - min);

  /** 第 k 顆按鈕（0＝1 號，順時針）的方向上、離圓心 r 的位置。 */
  function button(k: number, r: number): [number, number] {
    const angle = STEP + 2 * STEP * k;
    return [r * Math.sin(angle), -r * Math.cos(angle)];
  }

  /** 這個姿勢下，兩點的圓心離盤面中心最遠多少（中點位移乘上 scale）。 */
  function reach(p: Pose, scale = 1): number {
    const rad = (p.deg * Math.PI) / 180;
    const dx = HALF * Math.cos(rad);
    const dy = HALF * Math.sin(rad);
    return Math.max(
      Math.hypot(p.x * scale - dx, p.y * scale - dy),
      Math.hypot(p.x * scale + dx, p.y * scale + dy),
    );
  }

  /** 兩點會壓到圈的話，把中點往圓心收，收到剛好不超過 LIMIT（中點在圓心時兩點離中心只有 HALF，一定收得回來）。 */
  function fit(p: Pose): Pose {
    if (reach(p) <= LIMIT) return p;
    let lo = 0;
    let hi = 1;
    for (let i = 0; i < 12; i++) {
      const mid = (lo + hi) / 2;
      if (reach(p, mid) <= LIMIT) lo = mid;
      else hi = mid;
    }
    return { x: p.x * lo, y: p.y * lo, deg: p.deg };
  }

  /** 從 from 直接移到 to 的途中，兩點會不會靠圈太近（挑目的地用：選不必被 fit 收回來的路，動作才順）。 */
  function clear(from: Pose, to: Pose): boolean {
    for (let i = 1; i < 24; i++) {
      const p = i / 24;
      const at = {
        x: from.x + (to.x - from.x) * p,
        y: from.y + (to.y - from.y) * p,
        deg: from.deg + (to.deg - from.deg) * p,
      };
      if (reach(at) > LIMIT) return false;
    }
    return true;
  }

  function pick(prev: Pose): Pose {
    for (let attempt = 0; attempt < 200; attempt++) {
      let next: Pose;
      if (Math.random() < 0.35) {
        // 兩點一起靠到圈邊：以某顆按鈕、或兩顆按鈕中間的方向為中線，兩點在它兩側、都貼近圈。
        const mid = STEP * Math.floor(Math.random() * 16);
        const spread = Math.asin(HALF / NEAR);
        const at = (angle: number): [number, number] => [NEAR * Math.sin(angle), -NEAR * Math.cos(angle)];
        const a = at(mid - spread);
        const b = at(mid + spread);
        const [left, right] = a[0] <= b[0] ? [a, b] : [b, a];
        next = {
          x: (left[0] + right[0]) / 2,
          y: (left[1] + right[1]) / 2,
          deg: (Math.atan2(right[1] - left[1], right[0] - left[0]) * 180) / Math.PI,
        };
      } else {
        // 其中一點靠向某顆按鈕（貼近圈）、或在盤面內任意一點；另一點在固定距離外、不交叉的方向上。
        const deg = rand(-90, 90);
        const ux = Math.cos((deg * Math.PI) / 180);
        const uy = Math.sin((deg * Math.PI) / 180);
        let anchor: [number, number];
        if (Math.random() < 0.6) {
          anchor = button(Math.floor(Math.random() * 8), NEAR);
        } else {
          const r = NEAR * Math.sqrt(Math.random());
          const angle = rand(0, 2 * Math.PI);
          anchor = [r * Math.cos(angle), r * Math.sin(angle)];
        }
        // side 1：落點是粉點（藍點在它的 +方向）；−1：落點是藍點。
        const side = Math.random() < 0.5 ? 1 : -1;
        const other: [number, number] = [anchor[0] + side * 2 * HALF * ux, anchor[1] + side * 2 * HALF * uy];
        if (Math.hypot(other[0], other[1]) > NEAR) continue;
        next = { x: anchor[0] + side * HALF * ux, y: anchor[1] + side * HALF * uy, deg };
      }
      // 太近的不算一次移動。
      if (Math.hypot(next.x - prev.x, next.y - prev.y) < 8) continue;
      // 過去的路上、以及之後回原位的路上都不靠圈太近。
      if (!clear(prev, next) || !clear(next, HOME)) continue;
      return next;
    }
    return HOME;
  }

  /** 從現在的位置移到 to，花 ms 毫秒；到了之後呼叫 done。再呼叫一次就從當下的位置改道。 */
  function go(to: Pose, ms: number, done?: () => void) {
    anim = { from: pose, to, start: performance.now(), ms: Math.max(1, ms), done };
    if (!frame) frame = requestAnimationFrame(tick);
  }

  function tick(now: number) {
    frame = 0;
    const a = anim;
    if (!a) return;
    const p = Math.min(1, Math.max(0, (now - a.start) / a.ms));
    // 先快後慢（ease-out）。
    const e = 1 - (1 - p) ** 3;
    pose = fit({
      x: a.from.x + (a.to.x - a.from.x) * e,
      y: a.from.y + (a.to.y - a.from.y) * e,
      deg: a.from.deg + (a.to.deg - a.from.deg) * e,
    });
    if (p < 1) {
      frame = requestAnimationFrame(tick);
      return;
    }
    if (anim === a) anim = null;
    a.done?.();
  }

  /** 走四次再回原位。 */
  function roam() {
    if (roaming || reduced) return;
    roaming = true;
    let step = 0;
    const move = () => {
      if (!roaming) return;
      const last = step === MOVES;
      step += 1;
      go(last ? HOME : pick(pose), rand(MIN_MS, MAX_MS), () => {
        if (!roaming) return;
        if (last) roaming = false;
        else roamTimer = window.setTimeout(move, rand(120, 380));
      });
    };
    move();
  }

  function stopRoam() {
    roaming = false;
    window.clearTimeout(roamTimer);
  }

  /**
   * 滑鼠在附近：兩點保持水平（像眼睛），往游標的反方向躲。
   * 只躲到八個固定的位置：反方向對到最近的那顆按鈕，往那個方向走到底。
   */
  function onNear(event: PointerEvent) {
    if (reduced || event.pointerType !== 'mouse' || pressed || rubbing) return;
    const box = root.getBoundingClientRect();
    const dx = event.clientX - (box.left + box.width / 2);
    const dy = event.clientY - (box.top + box.height / 2);
    const distance = Math.hypot(dx, dy);
    if (distance >= SENSE) {
      if (dodging) {
        dodging = false;
        dodgeSlot = -1;
        go(HOME, 500);
      }
      return;
    }
    if (performance.now() < holdUntil) return;
    // 反方向的角度：從正上方順時針量，0～2π。游標剛好在正中間時沿用原本的位置（沒有就往上）。
    const away = distance > 0.5 ? (Math.atan2(-dx, dy) + 2 * Math.PI) % (2 * Math.PI) : -1;
    let slot = dodgeSlot;
    if (away < 0) {
      if (slot < 0) slot = 0;
    } else if (slot < 0) {
      slot = Math.floor(away / (2 * STEP)) % 8;
    } else {
      // 已經躲在某個位置：角度要明顯越過交界才換，游標在交界附近時不會兩邊跳。
      const center = STEP + 2 * STEP * slot;
      const off = Math.abs(((away - center + 3 * Math.PI) % (2 * Math.PI)) - Math.PI);
      if (off > STEP + 0.14) slot = Math.floor(away / (2 * STEP)) % 8;
    }
    if (dodging && slot === dodgeSlot) return;
    stopRoam();
    dodging = true;
    dodgeSlot = slot;
    const [ax, ay] = button(slot, 1);
    // 兩點水平時，中點往 (ax, ay) 最遠能走多遠而兩點都還在 NEAR 以內。
    const far = -HALF * Math.abs(ax) + Math.sqrt(HALF * HALF * ax * ax + NEAR * NEAR - HALF * HALF);
    go({ x: ax * far, y: ay * far, deg: 0 }, 260);
  }

  /** 點一下／摸一下：直接跑到上面，停一下再回來。 */
  function hop() {
    if (reduced) return;
    stopRoam();
    dodging = false;
    dodgeSlot = -1;
    window.clearTimeout(hopTimer);
    holdUntil = performance.now() + 220 + 650;
    go(UP, 220, () => {
      hopTimer = window.setTimeout(() => go(HOME, 520), 650);
    });
  }

  /** 搓完了：睜開眼，移回正中間。 */
  function recover() {
    rubbing = false;
    if (!reduced) go(HOME, 480);
  }

  /** 放開或停下來之後，先維持一下再恢復。 */
  function settleRub(ms: number) {
    window.clearTimeout(rubTimer);
    rubTimer = window.setTimeout(recover, ms);
  }

  function startRub() {
    if (!rubbing) {
      rubbing = true;
      stopRoam();
      dodging = false;
      dodgeSlot = -1;
      window.clearTimeout(hopTimer);
    }
    // 按著不動也算停下來。
    settleRub(RUB_HOLD);
  }

  function onDown(event: PointerEvent) {
    pressed = true;
    lastX = event.clientX;
    dir = 0;
    stroke = 0;
    moved = 0;
    turns = [];
    // 量裡面的 svg（外層多了一圈感應範圍）。
    const box = (root.firstElementChild ?? root).getBoundingClientRect();
    centerX = box.left + box.width / 2;
    unit = box.width / 100 || 1;
    // 手指滑出標誌外也繼續收到移動。抓不到（例如這個指標已經不在了）就算了，不影響其他互動。
    try {
      root.setPointerCapture(event.pointerId);
    } catch {
      // 忽略
    }
  }

  function onDrag(event: PointerEvent) {
    if (!pressed) return;
    const dx = event.clientX - lastX;
    lastX = event.clientX;
    if (!dx) return;
    moved += Math.abs(dx);
    // 正在被搓：眼睛跟著手指左右移（只有水平平移，手指離中心約半個標誌寬就到底）。
    if (rubbing && !reduced) {
      const x = Math.max(-SIDE, Math.min(SIDE, ((event.clientX - centerX) / unit) * 0.35));
      go({ x, y: 0, deg: 0 }, 90);
    }
    const sign = Math.sign(dx);
    if (sign === dir) {
      stroke += Math.abs(dx);
      return;
    }
    // 換方向：前一段夠長（不是手抖）才算一次來回。
    if (dir !== 0 && stroke >= 4) {
      const now = performance.now();
      turns = [...turns.filter((time) => now - time < 700), now];
      if (turns.length >= 2) startRub();
    }
    dir = sign;
    stroke = Math.abs(dx);
  }

  function onUp(event: PointerEvent) {
    if (!pressed) return;
    pressed = false;
    try {
      root.releasePointerCapture(event.pointerId);
    } catch {
      // 忽略
    }
    if (rubbing) {
      settleRub(RUB_HOLD);
      return;
    }
    if (moved < 10) hop();
  }

  function onCancel() {
    pressed = false;
    if (rubbing) settleRub(RUB_HOLD);
  }

  onMount(() => {
    reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
    let start = 0;
    let observer: IntersectionObserver | undefined;
    if (!reduced) {
      document.addEventListener('pointermove', onNear, { passive: true });
      if ('IntersectionObserver' in window) {
        observer = new IntersectionObserver(
          (entries) => {
            if (!entries.some((entry) => entry.isIntersecting)) return;
            observer?.disconnect();
            start = window.setTimeout(() => {
              if (!dodging && !rubbing && !pressed) roam();
            }, 700);
          },
          { threshold: 0.6 },
        );
        observer.observe(root);
      }
    }
    return () => {
      document.removeEventListener('pointermove', onNear);
      observer?.disconnect();
      cancelAnimationFrame(frame);
      for (const id of [start, roamTimer, hopTimer, rubTimer]) window.clearTimeout(id);
    };
  });
</script>

<!-- 純裝飾的小互動（躲游標、點一下往上跑、搓一搓瞇眼），沒有任何功能，所以不做成按鈕、也不進輔助技術。 -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<span
  class={['roam', rubbing && 'is-rubbing']}
  aria-hidden="true"
  bind:this={root}
  onpointerdown={onDown}
  onpointermove={onDrag}
  onpointerup={onUp}
  onpointercancel={onCancel}
>
  <svg class="roam-logo" width={size} height={size} viewBox="0 0 100 100">
    <circle class="roam-ring" cx="50" cy="50" r="38" />
    <g
      class="roam-hands"
      transform={`translate(${pose.x.toFixed(2)} ${pose.y.toFixed(2)}) rotate(${pose.deg.toFixed(2)} 50 50)`}
    >
      {#if rubbing}
        <!-- 被搓的時候眼睛瞇成「> <」。 -->
        <path class="roam-squint roam-squint--left" d="M29 41 L43 50 L29 59" />
        <path class="roam-squint roam-squint--right" d="M71 41 L57 50 L71 59" />
      {:else}
        <circle class="roam-left" cx="36" cy="50" r="10" />
        <circle class="roam-right" cx="64" cy="50" r="10" />
      {/if}
    </g>
  </svg>
</span>

<style>
  /* 外面多一圈當作感應範圍（用負的 margin 抵掉，不影響版面）。左右搓要收得到移動，所以只把上下捲動留給瀏覽器。 */
  .roam {
    display: inline-flex;
    flex: none;
    margin: calc(var(--space-2) * -1);
    padding: var(--space-2);
    color: var(--c-text-strong);
    touch-action: pan-y;
    user-select: none;
    -webkit-user-select: none;
    -webkit-tap-highlight-color: transparent;
  }

  .roam-logo {
    display: block;
    overflow: visible;
  }

  .roam-ring {
    fill: none;
    stroke: currentColor;
    stroke-width: 10.5;
  }

  .roam-left {
    fill: var(--c-left);
  }

  .roam-right {
    fill: var(--c-right);
  }

  .roam-squint {
    fill: none;
    stroke-width: 7;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .roam-squint--left {
    stroke: var(--c-left);
  }

  .roam-squint--right {
    stroke: var(--c-right);
  }
</style>
