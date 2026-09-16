<script lang="ts" module>
  /**
   * 線條圖示，外形取自 Lucide（ISC 授權），全部轉成 path 以便在 Svelte 內直接輸出。
   * 整個介面只用這一套，24×24、線寬 2、跟隨文字顏色。
   */
  const ICONS = {
    plus: ['M5 12h14', 'M12 5v14'],
    x: ['M18 6 6 18', 'm6 6 12 12'],
    play: ['M6 3 20 12 6 21Z'],
    pause: [
      'M14 5a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1h-2a1 1 0 0 1-1-1Z',
      'M6 5a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1Z',
    ],
    'skip-back': ['M19 20 9 12 19 4Z', 'M5 19V5'],
    'chevron-left': ['m15 18-6-6 6-6'],
    'chevron-right': ['m9 18 6-6-6-6'],
    'rotate-ccw': ['M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8', 'M3 3v5h5'],
    'refresh-cw': [
      'M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8',
      'M21 3v5h-5',
      'M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16',
      'M8 16H3v5',
    ],
    'to-start': ['M3 19V5', 'm13 6-6 6 6 6', 'M7 12h14'],
    'to-end': ['M17 12H3', 'm11 18 6-6-6-6', 'M21 5v14'],
    'move-horizontal': ['m18 8 4 4-4 4', 'M2 12h20', 'm6 8-4 4 4 4'],
    trash: [
      'M3 6h18',
      'M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6',
      'M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2',
    ],
    crosshair: [
      'M2 12a10 10 0 1 0 20 0a10 10 0 1 0-20 0',
      'M22 12h-4',
      'M6 12H2',
      'M12 6V2',
      'M12 22v-4',
    ],
    clock: ['M2 12a10 10 0 1 0 20 0a10 10 0 1 0-20 0', 'M12 6v6l4 2'],
    eye: [
      'M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z',
      'M9 12a3 3 0 1 0 6 0a3 3 0 1 0-6 0',
    ],
    sliders: [
      'M21 4h-7',
      'M10 4H3',
      'M21 12h-9',
      'M8 12H3',
      'M21 20h-5',
      'M12 20H3',
      'M14 2v4',
      'M8 10v4',
      'M16 18v4',
    ],
    compare: [
      'M15 18a3 3 0 1 0 6 0a3 3 0 1 0-6 0',
      'M3 6a3 3 0 1 0 6 0a3 3 0 1 0-6 0',
      'M13 6h3a2 2 0 0 1 2 2v7',
      'M11 18H8a2 2 0 0 1-2-2V9',
    ],
    'circle-dot': ['M2 12a10 10 0 1 0 20 0a10 10 0 1 0-20 0', 'M11 12a1 1 0 1 0 2 0a1 1 0 1 0-2 0'],
    loader: ['M21 12a9 9 0 1 1-6.219-8.56'],
    'circle-check': ['M2 12a10 10 0 1 0 20 0a10 10 0 1 0-20 0', 'm9 12 2 2 4-4'],
    'circle-alert': ['M2 12a10 10 0 1 0 20 0a10 10 0 1 0-20 0', 'M12 8v4', 'M12 16h.01'],
    'triangle-alert': [
      'm21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3',
      'M12 9v4',
      'M12 17h.01',
    ],
    info: ['M2 12a10 10 0 1 0 20 0a10 10 0 1 0-20 0', 'M12 16v-4', 'M12 8h.01'],
    'file-text': [
      'M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z',
      'M14 2v4a2 2 0 0 0 2 2h4',
      'M10 9H8',
      'M16 13H8',
      'M16 17H8',
    ],
    zap: [
      'M4 14a1 1 0 0 1-.78-1.63l9.9-10.2a.5.5 0 0 1 .86.46l-1.92 6.02A1 1 0 0 0 13 10h7a1 1 0 0 1 .78 1.63l-9.9 10.2a.5.5 0 0 1-.86-.46l1.92-6.02A1 1 0 0 0 11 14Z',
    ],
    'text-cursor': ['M17 22h-1a4 4 0 0 1-4-4V6a4 4 0 0 1 4-4h1', 'M7 22h1a4 4 0 0 0 4-4v-1', 'M7 2h1a4 4 0 0 1 4 4v1'],
  } as const;

  export type IconName = keyof typeof ICONS;
</script>

<script lang="ts">
  interface Props {
    name: IconName;
    size?: number;
    spin?: boolean;
  }

  let { name, size = 15, spin = false }: Props = $props();
</script>

<svg
  class="icon"
  class:icon--spin={spin}
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  {#each ICONS[name] as d (d)}
    <path {d} />
  {/each}
</svg>

<style>
  .icon {
    flex: none;
    display: block;
  }

  .icon--spin {
    animation: icon-spin 0.9s linear infinite;
  }

  @keyframes icon-spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .icon--spin {
      animation-duration: 2.4s;
    }
  }
</style>
