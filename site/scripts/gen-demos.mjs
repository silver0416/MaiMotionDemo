// 盤面演示資料：對每一段 simai 呼叫 Rust 核心（target/release/mai-motion-core.exe），
// 以 App 的預設參數、評分方式固定 V4.3 算出左右手，精簡後寫到 src/lib/demos/data/<id>.json。
// 前端只播放這些資料，不重算左右手。
//
//   npm.cmd run demos            全部重算
//   npm.cmd run demos -- swap    只重算指定的 id
//
// 同一份核心與輸入一定得到同一份輸出（requestId 固定、數字固定四捨五入到 4 位小數、key 順序固定）。

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const siteRoot = join(here, '..');
const repoRoot = join(siteRoot, '..');
const outDir = join(siteRoot, 'src', 'lib', 'demos', 'data');
const exe = join(repoRoot, 'target', 'release', process.platform === 'win32' ? 'mai-motion-core.exe' : 'mai-motion-core');

// App 的預設參數：直接沿用前端的投影函式。評分方式固定用基座 V4.3：App 的預設 2026-11-24 起是 TemotoAI-2 浅葱，
// 但這些演示說明的是規則與成本，不跟著 AI 模型重訓而變。重算後若和浅葱的結果不同，對應文件要寫明是 V4.3
// （2026-11-24 對過：只有 abab 與 stair 不同，model/taps.md 已註明）。
const contract = await import(pathToFileURL(join(repoRoot, 'frontend', 'src', 'lib', 'contract.ts')).href);
const solverConfig = contract.projectConfig(contract.withScoring(contract.DEFAULT_DRAFT, { kind: 'base', id: 'v4.3' }));

/** 與 site/README.md「盤面演示 id」表一致。 */
const DEMOS = [
  ['tap-alternate', '(120){4}1,8,2,7,3,6,4,5,E'],
  ['hold-other-hand', '(120){4}8h[4:4]/1,2,3,4,E'],
  ['slide-basic', '(120){4}1-5[4:3],8,7,6,E'],
  ['swap', '(120){4}1-5[4:1]/5-1[4:1],{16},,,1,E'],
  ['cross', '(120){4}1-5[4:1]/5-1[4:1],E'],
  ['slide-long', '(120){4}8>4[1##3],,,,,,,8,E'],
  ['jack', '(240){24}1,1,1,1,E'],
  ['abab', '(240){24}1,2,1,2,1,2,E'],
  ['abab-far', '(240){24}1,8,1,8,1,8,E'],
  ['stair', '(200){16}6,5,4,3,2,1,8,7,E'],
  ['palm', '(120){4}Ch[4:2]/B1/E1/7,E'],
  ['palm-hold', '(190){4}1h[4:15],,,Ch[1:3],,,,,B1,B8,B2,B7,B3,B6,B4,B5,E'],
  ['wifi', '(120){4}1w5[4:2],E'],
  ['wifi-one-hand', '(120){4}1w5[4:2],{8},C,E'],
  ['slide-palm', '(120){4}1-5[4:1]/8h[4:3],{16},B2,E'],
  ['touch-kinds', '(120){4}A1f,B3,Cf,D5f,E7,Chf[4:2],E'],
  ['shapes', '(120){2}1-5[4:1],3^6[4:1],5v2[4:1],7q3[4:1],1s5[4:1],8w4[4:1],2V84[4:1],E'],
];

if (!existsSync(exe)) {
  console.error(`找不到 Rust 核心：${relative(process.cwd(), exe)}`);
  console.error('請先在 repo 根目錄執行：');
  console.error('  cargo build --release --bin mai-motion-core');
  process.exit(1);
}

const only = new Set(process.argv.slice(2));
for (const id of only) {
  if (!DEMOS.some(([name]) => name === id)) {
    console.error(`不認得的演示 id：${id}`);
    process.exit(1);
  }
}

/** 四捨五入到 4 位小數；-0 一律寫成 0，輸出才會位元組相同。 */
function round(value) {
  if (typeof value !== 'number' || !Number.isFinite(value)) return value;
  const r = Math.round(value * 1e4) / 1e4;
  return Object.is(r, -0) ? 0 : r;
}

function point(p) {
  return { x: round(p.x), y: round(p.y) };
}

function pick(object, keys) {
  const out = {};
  for (const key of keys) if (object[key] !== undefined) out[key] = object[key];
  return out;
}

/** 只留 true 的修飾；元件載入時補回其餘的 false。 */
function modifiers(m) {
  const out = {};
  for (const [key, value] of Object.entries(m ?? {})) if (value === true) out[key] = true;
  return out;
}

function compactNote(note) {
  return {
    id: note.id,
    kind: note.kind,
    button: note.button,
    touchArea: note.touchArea,
    timeSeconds: round(note.timeSeconds),
    endSeconds: round(note.endSeconds),
    position: point(note.position),
    pathId: note.pathId,
    motionStart: round(note.motionStart),
    motionEnd: round(note.motionEnd),
    hasHead: note.hasHead,
    modifiers: modifiers(note.modifiers),
  };
}

function compactSamples(samples) {
  return samples.map((s) => ({ u: round(s.u), x: round(s.x), y: round(s.y) }));
}

function compactPath(path) {
  return {
    id: path.id,
    shape: path.shape,
    startButton: path.startButton,
    endButton: path.endButton,
    samples: compactSamples(path.samples),
    branches: (path.branches ?? []).map(compactSamples),
  };
}

/**
 * 前後相接、模式與音符都相同的動作段（核心把一條 Slide 切成每 0.05 秒一段）併成一段。
 * 播放時查到的模式、音符與插值位置都和原本相同，只是少了重複的欄位。
 */
function mergeSegments(segments) {
  const out = [];
  for (const segment of segments) {
    const prev = out[out.length - 1];
    const first = segment.samples[0];
    const last = prev?.samples[prev.samples.length - 1];
    if (
      prev &&
      prev.mode === segment.mode &&
      prev.noteId === segment.noteId &&
      prev.endSeconds === segment.startSeconds &&
      first &&
      last &&
      last.timeSeconds === first.timeSeconds &&
      last.x === first.x &&
      last.y === first.y
    ) {
      prev.samples.push(...segment.samples.slice(1));
      prev.endSeconds = segment.endSeconds;
      continue;
    }
    out.push({ ...segment, samples: [...segment.samples] });
  }
  return out;
}

/**
 * 手的動作段：取樣點四捨五入，再拿掉落在前後兩點連線上的中間點（直線插值結果不變，
 * 誤差 < 0.0002 盤面半徑），檔案才壓得小。第一點與最後一點一定保留。
 * samples 存成 [時間, x, y]，元件載入時換回 { timeSeconds, x, y }。
 */
function compactSegment(segment) {
  const samples = segment.samples.map((s) => ({ t: round(s.timeSeconds), x: round(s.x), y: round(s.y) }));
  const kept = [];
  for (let i = 0; i < samples.length; i += 1) {
    const s = samples[i];
    const prev = kept[kept.length - 1];
    const next = samples[i + 1];
    if (prev && next && next.t > prev.t && s.t > prev.t && s.t < next.t) {
      const k = (s.t - prev.t) / (next.t - prev.t);
      const ex = prev.x + (next.x - prev.x) * k;
      const ey = prev.y + (next.y - prev.y) * k;
      if (Math.hypot(ex - s.x, ey - s.y) < 2e-4) continue;
    }
    if (prev && prev.t === s.t && prev.x === s.x && prev.y === s.y) continue;
    kept.push(s);
  }
  return {
    mode: segment.mode,
    noteId: segment.noteId,
    startSeconds: round(segment.startSeconds),
    endSeconds: round(segment.endSeconds),
    samples: kept.map((s) => [s.t, s.x, s.y]),
  };
}

function compactSolution(solution) {
  return {
    assignments: solution.assignments.map((a) => ({
      noteId: a.noteId,
      part: a.part,
      hand: a.hand,
      startSeconds: round(a.startSeconds),
      endSeconds: round(a.endSeconds),
    })),
    handovers: solution.handovers.map((h) => ({
      noteId: h.noteId,
      from: h.from,
      to: h.to,
      startSeconds: round(h.startSeconds),
      endSeconds: round(h.endSeconds),
      swap: h.swap,
    })),
    palmPlacements: solution.palmPlacements.map((p) => ({
      hand: p.hand,
      center: point(p.center),
      radius: round(p.radius),
      startSeconds: round(p.startSeconds),
      endSeconds: round(p.endSeconds),
      coveredNoteIds: p.coveredNoteIds,
    })),
    leftSegments: mergeSegments(solution.leftSegments).map(compactSegment),
    rightSegments: mergeSegments(solution.rightSegments).map(compactSegment),
    warnings: solution.warnings,
    ...pick(solution, ['skipped']),
  };
}

function analyze(id, source) {
  const request = { requestId: id, source, firstSeconds: 0, solverConfig };
  const run = spawnSync(exe, [], {
    input: JSON.stringify(request),
    encoding: 'utf8',
    maxBuffer: 256 * 1024 * 1024,
    windowsHide: true,
  });
  if (run.error) throw run.error;
  if (run.status !== 0) {
    throw new Error(`${id}：核心結束碼 ${run.status}\n${run.stderr}`);
  }
  return JSON.parse(run.stdout);
}

// ---- 摘要 ----

const fmt = (t) => (t < 0 ? '-' : '') + Math.abs(t).toFixed(3);

function noteWhere(note, pathById) {
  if (note.touchArea) return `${note.touchArea}${note.button > 0 ? note.button : ''}`;
  if (note.kind === 'slide' && note.pathId) {
    const path = pathById.get(note.pathId);
    if (path) return `${path.startButton}${path.shape}${path.endButton}${note.hasHead ? '' : '（無起點）'}`;
  }
  return String(note.button);
}

function handsFor(noteId, solution) {
  const list = solution.assignments.filter((a) => a.noteId === noteId);
  if (list.length === 0) return '（沒有指派）';
  return list.map((a) => `${a.part}:${a.hand}@${fmt(a.startSeconds)}-${fmt(a.endSeconds)}`).join(' ');
}

function summary(id, source, data, bytes) {
  const lines = [];
  lines.push(`## ${id}  status=${data.status}  音符 ${data.chart?.notes.length ?? 0}  檔案 ${bytes} bytes`);
  lines.push(`   ${source}`);
  if (data.diagnostics?.length) {
    for (const d of data.diagnostics) lines.push(`   診斷 ${d.severity} ${d.code}：${d.message}`);
  }
  const solution = data.solution;
  if (!data.chart) return lines.join('\n');
  const pathById = new Map(data.chart.paths.map((p) => [p.id, p]));
  for (const note of data.chart.notes) {
    const time = note.endSeconds > note.timeSeconds ? `${fmt(note.timeSeconds)}–${fmt(note.endSeconds)}` : fmt(note.timeSeconds);
    const mods = Object.keys(note.modifiers);
    lines.push(
      `   ${note.id.padEnd(4)} ${time.padEnd(13)} ${note.kind.padEnd(9)} ${noteWhere(note, pathById).padEnd(6)}${mods.length ? ` [${mods.join(',')}]` : ''}  → ${solution ? handsFor(note.id, solution) : '-'}`,
    );
  }
  if (solution) {
    if (solution.handovers.length === 0) lines.push('   交接／互換：無');
    for (const h of solution.handovers) {
      lines.push(`   ${h.swap ? '互換' : '交接'} ${h.noteId} ${h.from}→${h.to} ${fmt(h.startSeconds)}–${fmt(h.endSeconds)}`);
    }
    if (solution.palmPlacements.length === 0) lines.push('   手掌覆蓋：無');
    for (const p of solution.palmPlacements) {
      const names = p.coveredNoteIds.map((nid) => {
        const note = data.chart.notes.find((n) => n.id === nid);
        return note ? noteWhere(note, pathById) : nid;
      });
      lines.push(
        `   手掌 ${p.hand} 掌心(${p.center.x},${p.center.y}) r=${p.radius} ${fmt(p.startSeconds)}–${fmt(p.endSeconds)} 覆蓋 ${names.join('・')}`,
      );
    }
    if (solution.skipped?.length) lines.push(`   略過：${JSON.stringify(solution.skipped)}`);
    for (const w of solution.warnings) lines.push(`   warning：${w}`);
  } else {
    lines.push('   （沒有方案）');
  }
  return lines.join('\n');
}

mkdirSync(outDir, { recursive: true });
const report = [];
const problems = [];

for (const [id, source] of DEMOS) {
  if (only.size > 0 && !only.has(id)) continue;
  const response = analyze(id, source);
  const first = response.solutions?.[0] ?? null;
  const data = {
    id,
    source,
    status: response.status,
    ...(response.status !== 'ok'
      ? { diagnostics: response.diagnostics.map((d) => pick(d, ['code', 'severity', 'message', 'timeSeconds'])) }
      : {}),
    chart: response.chart
      ? {
          durationSeconds: round(response.chart.durationSeconds),
          notes: response.chart.notes.map(compactNote),
          paths: response.chart.paths.map(compactPath),
          // 33 個落點只在有 Touch 的演示用得到（自動顯示落點參考）。
          touchSensors: response.chart.notes.some((n) => n.kind === 'touch' || n.kind === 'touchHold')
            ? response.chart.touchSensors.map((s) => ({ area: s.area, index: s.index, position: point(s.position) }))
            : [],
        }
      : null,
    solution: first ? compactSolution(first) : null,
  };
  const text = `${JSON.stringify(data)}\n`;
  const file = join(outDir, `${id}.json`);
  writeFileSync(file, text);
  const bytes = Buffer.byteLength(text);
  report.push(summary(id, source, data, bytes));
  if (data.status !== 'ok') problems.push(`${id}：status ${data.status}`);
  if (bytes > 20 * 1024) problems.push(`${id}：${bytes} bytes，超過 20 KB`);
}

console.log(report.join('\n\n'));
if (problems.length) {
  console.log('\n注意：');
  for (const line of problems) console.log(`  ${line}`);
}
