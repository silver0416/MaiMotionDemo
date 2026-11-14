// 搜尋評估：用 scripts/search-queries.json 比較各 embedding 模型，以及字面／語意／合併三種排序。
//
//   node scripts/eval-search.mjs                    全部模型（向量有快取，只算沒算過的）
//   node scripts/eval-search.mjs --model <id>       只看一個模型
//   node scripts/eval-search.mjs --latency          另外量查詢延遲（每個模型 5 次單筆呼叫，不走快取；結果存 .cache/eval-latency.json）
//   node scripts/eval-search.mjs --verbose          列出每一筆查詢的前三名
//
// 命中的定義：結果依塊排序後，換成頁面並去重；任一個 expect 頁出現在第 1／前 3／前 5 名就算。
// 字面與合併直接呼叫 src/lib/search/engine.ts 的 combine()（和網頁同一份程式），語意結果用 int8 量化後的向量模擬 Worker。

import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import {
  MODELS,
  EMBED_MODEL,
  ROOT_DIR,
  callModel,
  collectChunks,
  embed,
  embeddingInput,
  normalizeVector,
  quantize,
} from './build-index.mjs';

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const option = (name) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : undefined;
};

/** Worker 回傳的語意結果筆數上限（和 worker/src/index.ts 一致）。 */
const TOP_K = 12;

const { queries } = JSON.parse(await readFile(join(ROOT_DIR, 'scripts', 'search-queries.json'), 'utf8'));
const positives = queries.filter((item) => !item.negative);
const negatives = queries.filter((item) => item.negative);
const chunks = await collectChunks();

// engine.ts 在瀏覽器裡 fetch('/search/chunks.json')；這裡換成記憶體裡的切塊結果。
const realFetch = globalThis.fetch;
globalThis.fetch = async (url, init) => {
  if (url === '/search/chunks.json') {
    return new Response(JSON.stringify({ version: 'eval', chunks }), { headers: { 'content-type': 'application/json' } });
  }
  return realFetch(url, init);
};
const engine = await import('../src/lib/search/engine.ts');
await engine.ensureIndex();

const pageOf = (id) => id.split('#')[0];

/** 依序換成頁面、去重，回傳第一個 expect 頁的名次（1 起算；沒有則 Infinity）。 */
function rankOf(ids, expect) {
  const pages = [];
  for (const id of ids) {
    const page = pageOf(id);
    if (!pages.includes(page)) pages.push(page);
  }
  const at = pages.findIndex((page) => expect.includes(page));
  return at < 0 ? Infinity : at + 1;
}

function summarize(ranks) {
  const n = ranks.length;
  const pct = (k) => ((ranks.filter((r) => r <= k).length / n) * 100).toFixed(1);
  const mrr = ranks.reduce((sum, r) => sum + (Number.isFinite(r) ? 1 / r : 0), 0) / n;
  return { top1: pct(1), top3: pct(3), top5: pct(5), mrr: mrr.toFixed(3) };
}

function dot(a, b) {
  let sum = 0;
  for (let i = 0; i < a.length; i += 1) sum += a[i] * b[i];
  return sum;
}

/** int8 量化後再還原（模擬 Worker 讀 vectors.bin）。 */
function dequantized(vector) {
  const { scale, q } = quantize(vector);
  return Array.from(q, (v) => v * scale);
}

function rankSemantic(queryVector, docVectors) {
  const scored = docVectors.map((vector, i) => ({ id: chunks[i].id, score: dot(queryVector, vector) }));
  scored.sort((a, b) => b.score - a.score);
  return scored;
}

// ---------------------------------------------------------------------------

const literalRanks = positives.map((item) => rankOf(engine.combine(item.q, null).map((hit) => hit.id), item.expect));
const literal = summarize(literalRanks);

const models = option('--model') ? [option('--model')] : Object.keys(MODELS);
const latencyPath = join(ROOT_DIR, '.cache', 'eval-latency.json');
let latency = {};
try {
  latency = JSON.parse(await readFile(latencyPath, 'utf8'));
} catch {}

// Worker 實際用的門檻（worker/src/index.ts 的 THRESHOLDS）。
const workerSource = await readFile(join(ROOT_DIR, 'worker', 'src', 'index.ts'), 'utf8');
const workerThresholds = Object.fromEntries(
  [...workerSource.matchAll(/"(@cf\/[^"]+)":\s*([\d.]+)/g)].map((m) => [m[1], Number(m[2])]),
);

const rows = [];
for (const model of models) {
  const docRaw = await embed(model, chunks.map(embeddingInput), 'document');
  const queryRaw = await embed(model, queries.map((item) => item.q), 'query');
  const dim = docRaw[0].length;
  const docs = docRaw.map(dequantized);
  const docsFloat = docRaw.map(normalizeVector);
  const qv = queryRaw.map(normalizeVector);

  const semanticLists = positives.map((item) => rankSemantic(qv[queries.indexOf(item)], docs));
  const semanticRanks = positives.map((item, i) => rankOf(semanticLists[i].map((r) => r.id), item.expect));

  // 量化的影響：float32 與 int8 的語意排名前 5 塊是否一樣。
  let sameTop5 = 0;
  positives.forEach((item, i) => {
    const a = rankSemantic(qv[queries.indexOf(item)], docsFloat).slice(0, 5).map((r) => r.id).join();
    const b = semanticLists[i].slice(0, 5).map((r) => r.id).join();
    if (a === b) sameTop5 += 1;
  });

  // 分數分布：正確頁最高分的那一塊 vs 無關查詢的第一名。
  const bestCorrect = semanticLists.map((list, i) => list.find((r) => positives[i].expect.includes(pageOf(r.id)))?.score ?? 0);
  const negTop = negatives.map((item) => rankSemantic(qv[queries.indexOf(item)], docs)[0].score);
  const sorted = [...bestCorrect].sort((a, b) => a - b);
  const quant = (p) => sorted[Math.min(sorted.length - 1, Math.floor(p * sorted.length))];

  // 門檻：在無關查詢最高分附近找，取合併後前 5 名最好的（同分取較高的門檻，雜訊較少）。
  const candidates = new Set([0]);
  for (let t = 0.2; t <= 0.8; t += 0.01) candidates.add(Number(t.toFixed(2)));
  if (workerThresholds[model] !== undefined) candidates.add(workerThresholds[model]);
  let best = null;
  const merged = {};
  for (const threshold of candidates) {
    const ranks = positives.map((item, i) => {
      const semantic = semanticLists[i].filter((r) => r.score >= threshold).slice(0, TOP_K);
      return rankOf(engine.combine(item.q, semantic).map((hit) => hit.id), item.expect);
    });
    const summary = summarize(ranks);
    merged[threshold] = { summary, ranks };
    const key = [Number(summary.top5), Number(summary.top3), Number(summary.top1), threshold];
    if (
      !best ||
      key[0] > best.key[0] ||
      (key[0] === best.key[0] && (key[1] > best.key[1] || (key[1] === best.key[1] && (key[2] > best.key[2] || (key[2] === best.key[2] && key[3] > best.key[3])))))
    ) {
      // 門檻不可以高到把正確結果砍掉太多：只在至少保留 90% 正確頁最高分的範圍內選。
      if (threshold <= quant(0.1)) best = { key, threshold };
    }
  }

  if (flag('--latency') && !latency[model]) {
    const times = [];
    for (const item of positives.slice(0, 5)) {
      const { ms } = await callModel(model, [`${item.q} ${Date.now()}`], 'query');
      times.push(ms);
    }
    times.sort((a, b) => a - b);
    latency[model] = { medianMs: Math.round(times[2]), samples: times.map(Math.round) };
    await mkdir(join(ROOT_DIR, '.cache'), { recursive: true });
    await writeFile(latencyPath, JSON.stringify(latency, null, 2));
  }

  const floatRanks = positives.map((item) => rankOf(rankSemantic(qv[queries.indexOf(item)], docsFloat).map((r) => r.id), item.expect));
  const returned = semanticLists.map((list) => list.filter((r) => r.score >= best.threshold).slice(0, TOP_K).length);
  const negReturned = negatives.map((item) => rankSemantic(qv[queries.indexOf(item)], docs).filter((r) => r.score >= best.threshold).length);

  rows.push({
    model,
    semanticFloat: summarize(floatRanks),
    avgReturned: (returned.reduce((a, b) => a + b, 0) / returned.length).toFixed(1),
    negReturned: negReturned.join('/'),
    negTop: negTop.map((s) => s.toFixed(3)).join('/'),
    dim,
    bytes: chunks.length * (dim + 4),
    semantic: summarize(semanticRanks),
    semanticRanks,
    merged0: merged[0].summary,
    mergedBest: merged[best.threshold].summary,
    workerThreshold: workerThresholds[model],
    mergedWorker: workerThresholds[model] !== undefined ? merged[workerThresholds[model]].summary : null,
    mergedRanks: merged[best.threshold].ranks,
    threshold: best.threshold,
    scores: {
      correctMin: sorted[0].toFixed(3),
      correctP10: quant(0.1).toFixed(3),
      correctMedian: quant(0.5).toFixed(3),
      negativeMax: Math.max(...negTop).toFixed(3),
    },
    sameTop5: `${sameTop5}/${positives.length}`,
    latency: latency[model]?.medianMs,
    lists: semanticLists,
  });
}

// ---------------------------------------------------------------------------
// 輸出

console.log(`\n查詢 ${positives.length} 筆（另有無關查詢 ${negatives.length} 筆只用來看門檻），${chunks.length} 塊。命中率為頁面層級（%）。\n`);
console.log('| 模型 | 方式 | 第 1 | 前 3 | 前 5 | MRR |');
console.log('| --- | --- | --- | --- | --- | --- |');
console.log(`| （不用模型） | 只有字面 | ${literal.top1} | ${literal.top3} | ${literal.top5} | ${literal.mrr} |`);
for (const row of rows) {
  const s = row.semantic;
  const m0 = row.merged0;
  const m = row.mergedBest;
  console.log(`| ${row.model} | 只有語意 | ${s.top1} | ${s.top3} | ${s.top5} | ${s.mrr} |`);
  console.log(`| ${row.model} | 合併（無門檻） | ${m0.top1} | ${m0.top3} | ${m0.top5} | ${m0.mrr} |`);
  console.log(`| ${row.model} | 合併（門檻 ${row.threshold}，評估最佳） | ${m.top1} | ${m.top3} | ${m.top5} | ${m.mrr} |`);
  const w = row.mergedWorker;
  if (w) console.log(`| ${row.model} | 合併（門檻 ${row.workerThreshold}，Worker 實際值） | ${w.top1} | ${w.top3} | ${w.top5} | ${w.mrr} |`);
}

console.log('\n| 模型 | 維度 | vectors.bin（int8） | 查詢延遲中位數（cf ai run，含 CLI 開銷） | 正確頁分數 最低／P10／中位 | 無關查詢最高分 | int8 與 float 前 5 相同 |');
console.log('| --- | --- | --- | --- | --- | --- | --- |');
for (const row of rows) {
  console.log(
    `| ${row.model} | ${row.dim} | ${(row.bytes / 1024).toFixed(0)} kB | ${row.latency ? `${row.latency} ms` : '未量'} | ${row.scores.correctMin}／${row.scores.correctP10}／${row.scores.correctMedian} | ${row.scores.negativeMax} | ${row.sameTop5} |`,
  );
}

console.log('\n| 模型 | 只有語意（float32，不量化） 第 1／前 3／前 5 | 門檻下平均回傳塊數 | 無關查詢各自的第一名分數 | 無關查詢在門檻上的塊數 |');
console.log('| --- | --- | --- | --- | --- |');
for (const row of rows) {
  const f = row.semanticFloat;
  console.log(`| ${row.model} | ${f.top1}／${f.top3}／${f.top5} | ${row.avgReturned} | ${row.negTop} | ${row.negReturned} |`);
}

// 合併有沒有比單獨語意差：逐筆列出變差的查詢。
for (const row of rows) {
  const worse = positives
    .map((item, i) => ({ q: item.q, semantic: row.semanticRanks[i], merged: row.mergedRanks[i] }))
    .filter((item) => item.merged > item.semantic);
  console.log(`\n${row.model}：合併比只有語意差的查詢 ${worse.length} 筆${worse.length ? '：' : ''}`);
  for (const item of worse) console.log(`  ${item.q}  語意第 ${item.semantic} → 合併第 ${item.merged}`);
}

const misses = (ranks) => positives.filter((_, i) => ranks[i] > 5).map((item) => item.q);
console.log(`\n只有字面、前 5 沒中：${misses(literalRanks).join('、') || '無'}`);
for (const row of rows) console.log(`${row.model} 合併、前 5 沒中：${misses(row.mergedRanks).join('、') || '無'}`);

if (flag('--verbose')) {
  for (const row of rows) {
    console.log(`\n=== ${row.model}（門檻 ${row.threshold}）`);
    positives.forEach((item, i) => {
      const semantic = row.lists[i].filter((r) => r.score >= row.threshold).slice(0, TOP_K);
      const hits = engine.combine(item.q, semantic).slice(0, 3);
      console.log(`${item.q}  →  ${hits.map((hit) => `${hit.id}(${hit.source})`).join('  ')}`);
    });
  }
}

console.log(`\n目前 build-index.mjs 的 EMBED_MODEL：${EMBED_MODEL}`);
