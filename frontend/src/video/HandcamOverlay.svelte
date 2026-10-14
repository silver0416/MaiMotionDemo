<script lang="ts">
  // 疊在影片上的盤面校正層：標點時接收點擊，標好後在參考格畫出推算的判定環與鍵號供目視確認。
  // 座標一律是影片原始像素（未翻轉）；影片翻轉顯示時，這層也跟著翻轉繪製，點擊換回原始座標。
  import type { Handcam } from './handcam.svelte';

  interface Props {
    handcam: Handcam;
    video: HTMLVideoElement;
    mirror: boolean;
    /** 影片目前時間 */
    current: number;
  }

  let { handcam, video, mirror, current }: Props = $props();

  let box = $state({ left: 0, top: 0, width: 0, height: 0 });
  let size = $state({ width: 0, height: 0 });

  function measure(): void {
    box = { left: video.offsetLeft, top: video.offsetTop, width: video.offsetWidth, height: video.offsetHeight };
    size = { width: video.videoWidth, height: video.videoHeight };
  }

  $effect(() => {
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(video);
    if (video.parentElement) observer.observe(video.parentElement);
    video.addEventListener('loadedmetadata', measure);
    return () => {
      observer.disconnect();
      video.removeEventListener('loadedmetadata', measure);
    };
  });

  const marking = $derived(handcam.phase === 'marking');
  const atReference = $derived(handcam.referenceTime !== null && Math.abs(current - handcam.referenceTime) < 0.02);
  const visible = $derived(marking || (atReference && handcam.pointCount > 0));

  const rings = $derived.by(() => {
    if (!visible || handcam.pointCount < 4) return [];
    return [1, 1.3].map((radius) => {
      const points: string[] = [];
      for (let k = 0; k <= 72; k += 1) {
        const a = (k / 72) * Math.PI * 2;
        const p = handcam.project(radius * Math.cos(a), radius * Math.sin(a));
        if (!p) return '';
        points.push(`${p.x.toFixed(1)},${p.y.toFixed(1)}`);
      }
      return points.join(' ');
    });
  });

  const labels = $derived.by(() => {
    if (!visible || handcam.pointCount < 4) return [];
    return Array.from({ length: 8 }, (_, index) => {
      const a = -Math.PI / 2 + Math.PI / 8 + index * (Math.PI / 4);
      return { button: index + 1, at: handcam.project(Math.cos(a), Math.sin(a)), label: handcam.project(1.15 * Math.cos(a), 1.15 * Math.sin(a)) };
    });
  });

  function onClick(event: MouseEvent): void {
    if (!marking || size.width === 0) return;
    const rect = (event.currentTarget as SVGSVGElement).getBoundingClientRect();
    const u = ((event.clientX - rect.left) / rect.width) * size.width;
    const v = ((event.clientY - rect.top) / rect.height) * size.height;
    handcam.place({ x: mirror ? size.width - u : u, y: v });
  }

  const scale = $derived(size.width > 0 ? size.width / Math.max(1, box.width) : 1);
</script>

{#if visible && size.width > 0}
  <svg
    class="overlay"
    class:is-marking={marking}
    style={`left:${box.left}px;top:${box.top}px;width:${box.width}px;height:${box.height}px`}
    viewBox={`0 0 ${size.width} ${size.height}`}
    onclick={onClick}
    role={marking ? 'button' : 'img'}
    aria-label={marking ? `點選 ${handcam.cursor + 1} 號鍵的環點` : '推算的判定環'}
  >
    <g transform={mirror ? `translate(${size.width} 0) scale(-1 1)` : undefined}>
      {#each rings as points, index (index)}
        {#if points}
          <polyline class="ring" class:ring--outer={index === 1} {points} fill="none" stroke-width={2 * scale} />
        {/if}
      {/each}
      {#each labels as item (item.button)}
        {#if item.at && item.label}
          <circle class="dot dot--projected" cx={item.at.x} cy={item.at.y} r={4 * scale} />
        {/if}
      {/each}
      {#each handcam.ring as point, index (index)}
        {#if point}
          <circle class="dot" cx={point.x} cy={point.y} r={6 * scale} stroke-width={2 * scale} />
        {/if}
      {/each}
    </g>
    <!-- 文字不翻轉，位置換算到顯示座標 -->
    {#each labels as item (item.button)}
      {#if item.label}
        <text
          class="label"
          x={mirror ? size.width - item.label.x : item.label.x}
          y={item.label.y}
          font-size={18 * scale}
          stroke-width={4 * scale}
        >{item.button}</text>
      {/if}
    {/each}
    {#each handcam.ring as point, index (index)}
      {#if point && handcam.pointCount < 4}
        <text class="label" x={(mirror ? size.width - point.x : point.x) + 10 * scale} y={point.y - 10 * scale} font-size={18 * scale} stroke-width={4 * scale}>{index + 1}</text>
      {/if}
    {/each}
  </svg>
{/if}

<style>
  .overlay {
    position: absolute;
    pointer-events: none;
  }

  .overlay.is-marking {
    pointer-events: auto;
    cursor: crosshair;
  }

  .ring {
    stroke: var(--c-accent);
  }

  .ring--outer {
    stroke-dasharray: 6 6;
  }

  .dot {
    fill: var(--c-right);
    stroke: var(--c-bg);
  }

  .dot--projected {
    fill: var(--c-accent);
  }

  .label {
    fill: var(--c-text);
    stroke: var(--c-bg);
    paint-order: stroke;
    font-family: var(--font-mono);
    font-weight: 700;
    text-anchor: middle;
    dominant-baseline: middle;
  }
</style>
