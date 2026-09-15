import tapFixture from '../../../fixtures/tap.json';
import holdFixture from '../../../fixtures/hold.json';
import slideFixture from '../../../fixtures/slide.json';
import handoverFixture from '../../../fixtures/handover.json';
import touchFixture from '../../../fixtures/touch.json';
import shapesFixture from '../../../fixtures/shapes.json';
import noSolutionFixture from '../../../fixtures/no-solution.json';
import invalidFixture from '../../../fixtures/invalid.json';
import { DEFAULT_CONFIG, asAnalyzeResponse, cloneConfig } from './contract';
import type { AnalyzeResponse, SolverConfig } from './types';

export interface Sample {
  id: string;
  title: string;
  description: string;
  source: string;
  firstSeconds: number;
  config: SolverConfig;
  /** fixtures 內附的核心輸出，僅供瀏覽器範例模式顯示。 */
  response: AnalyzeResponse;
}

interface RawFixture {
  request?: {
    source?: unknown;
    firstSeconds?: unknown;
    solverConfig?: unknown;
  };
  response?: unknown;
}

function build(
  id: string,
  title: string,
  description: string,
  raw: unknown,
): Sample {
  const fixture = (raw ?? {}) as RawFixture;
  const request = fixture.request ?? {};
  const source = typeof request.source === 'string' ? request.source : '';
  const firstSeconds = typeof request.firstSeconds === 'number' ? request.firstSeconds : 0;
  const config =
    typeof request.solverConfig === 'object' && request.solverConfig !== null
      ? ({ ...DEFAULT_CONFIG, ...(request.solverConfig as Partial<SolverConfig>) } as SolverConfig)
      : cloneConfig(DEFAULT_CONFIG);
  return {
    id,
    title,
    description,
    source,
    firstSeconds,
    config,
    response: asAnalyzeResponse(fixture.response, id),
  };
}

/** 由 `cargo run --offline --example fixtures` 產生的真實核心輸出。 */
export const SAMPLES: Sample[] = [
  build('tap', '基本 Tap', '八個外圈 Tap 依序落下。', tapFixture),
  build('hold', 'Tap 與 Hold', 'Hold 佔住一手，另一手連續移動。', holdFixture),
  build('slide', 'Slide 不交接', '直線 Slide 由單手完成。', slideFixture),
  build('handover', '可行交接', '慢速長圓弧，中途換手比較划算。', handoverFixture),
  build('touch', 'Touch 區', 'A／B／C／D／E 五種感應區與 Touch Hold。', touchFixture),
  build('shapes', 'Slide 形狀', '直線、圓弧、V、繞圈、S、Wifi、大 V。', shapesFixture),
  build('no-solution', '無方案', '三顆同時 Tap 超出雙手容量。', noSolutionFixture),
  build('invalid', '語法錯誤', '相鄰鍵位不能用直線 Slide，核心回報位置。', invalidFixture),
];

export function findSample(id: string | null): Sample | null {
  if (!id) return null;
  return SAMPLES.find((sample) => sample.id === id) ?? null;
}
