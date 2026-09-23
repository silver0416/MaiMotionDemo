<script lang="ts" module>
  /**
   * 線條圖示，外形取自 Lucide（ISC 授權），全部轉成 path 以便在 Svelte 內直接輸出。
   * 整個介面只用這一套，24×24、線寬 2、跟隨文字顏色。
   */
  const ICONS = {
    plus: ['M5 12h14', 'M12 5v14'],
    search: ['m21 21-4.34-4.34', 'M3 11a8 8 0 1 0 16 0a8 8 0 1 0-16 0'],
    download: ['M12 15V3', 'M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4', 'm7 10 5 5 5-5'],
    upload: ['M12 3v12', 'm17 8-5-5-5 5', 'M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4'],
    hand: [
      'M18 11V6a2 2 0 0 0-4 0v5',
      'M14 10V4a2 2 0 0 0-4 0v2',
      'M10 10.5V6a2 2 0 0 0-4 0v8',
      'M18 8a2 2 0 1 1 4 0v6a8 8 0 0 1-8 8h-2c-2.8 0-4.5-.86-5.99-2.34l-3.6-3.6a2 2 0 0 1 2.83-2.82L7 15',
    ],
    x: ['M18 6 6 18', 'm6 6 12 12'],
    copy: [
      'M8 10a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2Z',
      'M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2',
    ],
    check: ['M20 6 9 17l-5-5'],
    'external-link': ['M15 3h6v6', 'M10 14 21 3', 'M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6'],
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
    'bookmark-plus': ['m19 21-7-4-7 4V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v16z', 'M12 7v6', 'M15 10H9'],
    bookmark: ['m19 21-7-4-7 4V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v16z'],
    'more-horizontal': [
      'M11 12a1 1 0 1 0 2 0a1 1 0 1 0-2 0',
      'M18 12a1 1 0 1 0 2 0a1 1 0 1 0-2 0',
      'M4 12a1 1 0 1 0 2 0a1 1 0 1 0-2 0',
    ],
    pencil: [
      'M21.17 6.81a1 1 0 0 0-3.99-3.99L3.84 16.17a2 2 0 0 0-.5.83l-1.32 4.35a.5.5 0 0 0 .62.62l4.35-1.32a2 2 0 0 0 .83-.5Z',
      'm15 5 4 4',
    ],
    settings: [
      'M9.67 4.14a2.34 2.34 0 0 1 4.66 0 2.34 2.34 0 0 0 3.32 1.92 2.34 2.34 0 0 1 2.33 4.04 2.34 2.34 0 0 0 0 3.8 2.34 2.34 0 0 1-2.33 4.04 2.34 2.34 0 0 0-3.32 1.92 2.34 2.34 0 0 1-4.66 0 2.34 2.34 0 0 0-3.32-1.92 2.34 2.34 0 0 1-2.33-4.04 2.34 2.34 0 0 0 0-3.8 2.34 2.34 0 0 1 2.33-4.04A2.34 2.34 0 0 0 9.67 4.14',
      'M9 12a3 3 0 1 0 6 0a3 3 0 1 0-6 0',
    ],
    database: [
      'M3 5a9 3 0 1 0 18 0a9 3 0 1 0-18 0',
      'M3 5v14a9 3 0 0 0 18 0V5',
      'M3 12a9 3 0 0 0 18 0',
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
