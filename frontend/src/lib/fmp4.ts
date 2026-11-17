// 把分段的 mp4（fragmented MP4）改寫成一般的 mp4，給 Linux 的影片播放用（見 video.ts 的 fileSource）。
//
// yt-dlp 下載的 YouTube 影片是分段 mp4：moov 裡的樣本表是空的，樣本的位置與時間分散在每一段的 moof 裡。
// Linux 的 WebKitGTK 播 blob: 網址時，GStreamer 只能從頭依序讀檔（見 video.ts 的 videoPreload）；
// 這種讀法下 GStreamer 的 mp4 解析器不能在分段 mp4 裡跳轉，而改播放速度也是靠跳轉，所以兩者都會失敗。
// 一般的 mp4 在 moov 裡有完整的樣本表，就能跳轉。
//
// 這裡只重寫檔頭：新的 ftyp、補上樣本表並拿掉 mvex 的 moov，加上一個 mdat 把原檔每個 mdat 的內容依序接起來。
// 影片資料直接引用原本 Blob 的片段（Blob 的 slice），不複製；sidx、moof 等其他最上層的 box 都不再需要。
// 不是分段 mp4 就原樣回傳；是分段 mp4 但看不懂（結構不符預期）就丟出錯誤，由呼叫端決定改用原檔。

interface Box {
  type: string;
  /** box 開頭在所屬範圍（整個檔案或某個 DataView）裡的位置 */
  start: number;
  size: number;
  header: number;
}

interface Defaults {
  description: number;
  duration: number;
  size: number;
  flags: number;
}

interface Chunk {
  /** 原檔中第一個樣本的位置 */
  offset: number;
  /** 這一段資料的長度（樣本大小加總） */
  length: number;
  samples: number;
  description: number;
}

interface Track {
  id: number;
  /** mdhd 的時間單位 */
  timescale: number;
  trex: Defaults | undefined;
  dts: number[];
  durations: number[];
  sizes: number[];
  cts: number[];
  sync: boolean[];
  chunks: Chunk[];
}

/** 最上層的 box 最多讀這麼多個；超過就當成看不懂（避免異常檔案讓迴圈跑太久）。 */
const MAX_TOP_LEVEL_BOXES = 200_000;
const UINT32_MAX = 0xffffffff;

/** 分段 mp4 改寫成一般 mp4；不是分段 mp4 就原樣回傳。 */
export async function defragment(file: Blob): Promise<Blob> {
  const top = await topLevelBoxes(file);
  const moovBox = top.find((box) => box.type === 'moov');
  const moofBoxes = top.filter((box) => box.type === 'moof');
  if (!moovBox || moofBoxes.length === 0) return file;
  if (moofBoxes[0].start < moovBox.start) throw new Error('moof before moov');

  const moov = await read(file, moovBox.start, moovBox.start + moovBox.size);
  const movie = parseMoov(moov, moovBox.header);
  for (const box of moofBoxes) {
    const moof = await read(file, box.start, box.start + box.size);
    parseMoof(moof, box, movie.tracks);
  }

  // 原檔每個 mdat 的內容依序接成新的 mdat；樣本位置換算成在新 mdat 內容裡的位置
  const payloads = top
    .filter((box) => box.type === 'mdat')
    .map((box) => ({ start: box.start + box.header, end: box.start + box.size }));
  let payloadSize = 0;
  const payloadStarts = payloads.map((payload) => {
    const start = payloadSize;
    payloadSize += payload.end - payload.start;
    return start;
  });
  const locate = (chunk: Chunk): number => {
    const index = payloads.findIndex((payload) => chunk.offset >= payload.start && chunk.offset + chunk.length <= payload.end);
    if (index < 0) throw new Error(`sample data at ${chunk.offset} is outside every mdat`);
    return payloadStarts[index] + (chunk.offset - payloads[index].start);
  };
  const chunkPositions = new Map<number, number[]>();
  for (const track of movie.tracks.values()) {
    finishTiming(track);
    chunkPositions.set(track.id, track.chunks.map(locate));
  }

  const ftypBox = top.find((box) => box.type === 'ftyp');
  const ftyp = makeFtyp(ftypBox ? await read(file, ftypBox.start, ftypBox.start + ftypBox.size) : undefined, ftypBox?.header ?? 8);
  const mdatHeader = payloadSize + 8 > UINT32_MAX ? 16 : 8;
  // 位置表的大小只看筆數與寬度：先用 0 算出 moov 的大小，就知道資料從哪裡開始
  const buildWith = (base: number, wide: boolean) =>
    buildMoov(moov, moovBox.header, movie, (track) => chunkPositions.get(track.id)!.map((position) => base + position), wide);
  let wide = false;
  let base = ftyp.length + buildWith(0, wide).length + mdatHeader;
  if (base + payloadSize > UINT32_MAX) {
    wide = true;
    base = ftyp.length + buildWith(0, wide).length + mdatHeader;
  }
  const newMoov = buildWith(base, wide);

  const header = new DataView(new ArrayBuffer(mdatHeader));
  if (mdatHeader === 16) {
    header.setUint32(0, 1);
    setFourcc(header, 4, 'mdat');
    header.setBigUint64(8, BigInt(payloadSize + 16));
  } else {
    header.setUint32(0, payloadSize + 8);
    setFourcc(header, 4, 'mdat');
  }
  const parts: BlobPart[] = [ftyp, newMoov, new Uint8Array(header.buffer)];
  for (const payload of payloads) parts.push(file.slice(payload.start, payload.end));
  return new Blob(parts, { type: file.type || 'video/mp4' });
}

// ---- 讀取 ----

async function read(file: Blob, start: number, end: number): Promise<DataView> {
  return new DataView(await file.slice(start, end).arrayBuffer());
}

function fourcc(view: DataView, pos: number): string {
  return String.fromCharCode(view.getUint8(pos), view.getUint8(pos + 1), view.getUint8(pos + 2), view.getUint8(pos + 3));
}

function setFourcc(view: DataView, pos: number, type: string): void {
  for (let i = 0; i < 4; i += 1) view.setUint8(pos + i, type.charCodeAt(i));
}

function boxAt(view: DataView, pos: number, end: number): Box {
  let size = view.getUint32(pos);
  const type = fourcc(view, pos + 4);
  let header = 8;
  if (size === 1) {
    size = Number(view.getBigUint64(pos + 8));
    header = 16;
  } else if (size === 0) {
    size = end - pos;
  }
  if (size < header || pos + size > end) throw new Error(`bad ${type} box at ${pos}`);
  return { type, start: pos, size, header };
}

/** 最上層的 box：只讀每個 box 的開頭 16 bytes。 */
async function topLevelBoxes(file: Blob): Promise<Box[]> {
  const boxes: Box[] = [];
  let pos = 0;
  while (pos + 8 <= file.size) {
    if (boxes.length >= MAX_TOP_LEVEL_BOXES) throw new Error('too many boxes');
    const head = await read(file, pos, Math.min(pos + 16, file.size));
    const box = boxAt(head, 0, file.size - pos);
    boxes.push({ ...box, start: pos });
    pos += box.size;
  }
  return boxes;
}

function children(view: DataView, start: number, end: number): Box[] {
  const boxes: Box[] = [];
  for (let pos = start; pos + 8 <= end; ) {
    const box = boxAt(view, pos, end);
    boxes.push(box);
    pos += box.size;
  }
  return boxes;
}

function childrenOf(view: DataView, box: Box): Box[] {
  return children(view, box.start + box.header, box.start + box.size);
}

function find(boxes: Box[], type: string): Box | undefined {
  return boxes.find((box) => box.type === type);
}

/** 讀 32 或 64 位元的正整數（full box 的 version 1 欄位較寬）。 */
function getUint(view: DataView, pos: number, wide: boolean): number {
  return wide ? Number(view.getBigUint64(pos)) : view.getUint32(pos);
}

// ---- moov ----

interface Movie {
  timescale: number;
  tracks: Map<number, Track>;
}

function parseMoov(view: DataView, header: number): Movie {
  const boxes = children(view, header, view.byteLength);
  const mvhd = find(boxes, 'mvhd');
  if (!mvhd) throw new Error('no mvhd');
  const mvhdBody = mvhd.start + mvhd.header;
  const timescale = view.getUint32(mvhdBody + (view.getUint8(mvhdBody) === 1 ? 20 : 12));

  const trex = new Map<number, Defaults>();
  const mvex = find(boxes, 'mvex');
  for (const box of mvex ? childrenOf(view, mvex) : []) {
    if (box.type !== 'trex') continue;
    const body = box.start + box.header;
    trex.set(view.getUint32(body + 4), {
      description: view.getUint32(body + 8),
      duration: view.getUint32(body + 12),
      size: view.getUint32(body + 16),
      flags: view.getUint32(body + 20),
    });
  }

  const tracks = new Map<number, Track>();
  for (const trak of boxes.filter((box) => box.type === 'trak')) {
    const parts = childrenOf(view, trak);
    const tkhd = find(parts, 'tkhd');
    const mdia = find(parts, 'mdia');
    if (!tkhd || !mdia) throw new Error('trak without tkhd or mdia');
    const tkhdBody = tkhd.start + tkhd.header;
    const id = view.getUint32(tkhdBody + (view.getUint8(tkhdBody) === 1 ? 20 : 12));
    const mdiaParts = childrenOf(view, mdia);
    const mdhd = find(mdiaParts, 'mdhd');
    const minf = find(mdiaParts, 'minf');
    if (!mdhd || !minf) throw new Error('mdia without mdhd or minf');
    const mdhdBody = mdhd.start + mdhd.header;
    const stbl = find(childrenOf(view, minf), 'stbl');
    if (!stbl) throw new Error('minf without stbl');
    // moov 裡已經有樣本的混合檔案不處理
    for (const box of childrenOf(view, stbl)) {
      const body = box.start + box.header;
      if ((box.type === 'stts' || box.type === 'stco' || box.type === 'co64') && view.getUint32(body + 4) > 0) {
        throw new Error('moov already has samples');
      }
      if (box.type === 'stsz' && view.getUint32(body + 8) > 0) throw new Error('moov already has samples');
    }
    tracks.set(id, {
      id,
      timescale: view.getUint32(mdhdBody + (view.getUint8(mdhdBody) === 1 ? 20 : 12)),
      trex: trex.get(id),
      dts: [],
      durations: [],
      sizes: [],
      cts: [],
      sync: [],
      chunks: [],
    });
  }
  if (tracks.size === 0) throw new Error('no tracks');
  return { timescale, tracks };
}

// ---- moof ----

const TFHD_BASE_DATA_OFFSET = 0x1;
const TFHD_DESCRIPTION_INDEX = 0x2;
const TFHD_DEFAULT_DURATION = 0x8;
const TFHD_DEFAULT_SIZE = 0x10;
const TFHD_DEFAULT_FLAGS = 0x20;
const TFHD_DEFAULT_BASE_IS_MOOF = 0x20000;
const TRUN_DATA_OFFSET = 0x1;
const TRUN_FIRST_SAMPLE_FLAGS = 0x4;
const TRUN_DURATION = 0x100;
const TRUN_SIZE = 0x200;
const TRUN_FLAGS = 0x400;
const TRUN_CTS = 0x800;
const SAMPLE_IS_NON_SYNC = 0x10000;

/** moof 的內容；moof.start 是在原檔中的位置。 */
function parseMoof(view: DataView, moof: Box, tracks: Map<number, Track>): void {
  // 沒有指定 base data offset 時：第一個 traf 從 moof 開頭算，之後的接在前一個 traf 的資料後面
  let previousEnd = moof.start;
  for (const traf of children(view, moof.header, view.byteLength).filter((box) => box.type === 'traf')) {
    const parts = childrenOf(view, traf);
    const tfhd = find(parts, 'tfhd');
    if (!tfhd) throw new Error('traf without tfhd');
    let pos = tfhd.start + tfhd.header;
    const flags = view.getUint32(pos) & 0xffffff;
    const track = tracks.get(view.getUint32(pos + 4));
    if (!track) throw new Error(`traf for unknown track ${view.getUint32(pos + 4)}`);
    pos += 8;
    let base = flags & TFHD_DEFAULT_BASE_IS_MOOF ? moof.start : previousEnd;
    if (flags & TFHD_BASE_DATA_OFFSET) {
      base = Number(view.getBigUint64(pos));
      pos += 8;
    }
    const fallback: Defaults = track.trex ?? { description: 1, duration: 0, size: 0, flags: 0 };
    const defaults: Defaults = { ...fallback };
    if (flags & TFHD_DESCRIPTION_INDEX) {
      defaults.description = view.getUint32(pos);
      pos += 4;
    }
    if (flags & TFHD_DEFAULT_DURATION) {
      defaults.duration = view.getUint32(pos);
      pos += 4;
    }
    if (flags & TFHD_DEFAULT_SIZE) {
      defaults.size = view.getUint32(pos);
      pos += 4;
    }
    if (flags & TFHD_DEFAULT_FLAGS) defaults.flags = view.getUint32(pos);

    // 沒有 tfdt 就接在這個軌前一段的後面
    const tfdt = find(parts, 'tfdt');
    let dts = track.dts.length ? track.dts[track.dts.length - 1] + track.durations[track.durations.length - 1] : 0;
    if (tfdt) {
      const body = tfdt.start + tfdt.header;
      dts = getUint(view, body + 4, view.getUint8(body) === 1);
    }

    let dataEnd = base;
    for (const trun of parts.filter((box) => box.type === 'trun')) {
      let at = trun.start + trun.header;
      const version = view.getUint8(at);
      const runFlags = view.getUint32(at) & 0xffffff;
      const count = view.getUint32(at + 4);
      at += 8;
      let offset = dataEnd;
      if (runFlags & TRUN_DATA_OFFSET) {
        offset = base + view.getInt32(at);
        at += 4;
      }
      let firstFlags: number | undefined;
      if (runFlags & TRUN_FIRST_SAMPLE_FLAGS) {
        firstFlags = view.getUint32(at);
        at += 4;
      }
      const chunk: Chunk = { offset, length: 0, samples: count, description: defaults.description };
      for (let i = 0; i < count; i += 1) {
        let duration = defaults.duration;
        let size = defaults.size;
        let sampleFlags = defaults.flags;
        let cts = 0;
        if (runFlags & TRUN_DURATION) {
          duration = view.getUint32(at);
          at += 4;
        }
        if (runFlags & TRUN_SIZE) {
          size = view.getUint32(at);
          at += 4;
        }
        if (runFlags & TRUN_FLAGS) {
          sampleFlags = view.getUint32(at);
          at += 4;
        }
        if (i === 0 && firstFlags !== undefined) sampleFlags = firstFlags;
        if (runFlags & TRUN_CTS) {
          cts = version === 0 ? view.getUint32(at) : view.getInt32(at);
          at += 4;
        }
        track.dts.push(dts);
        track.durations.push(duration);
        track.sizes.push(size);
        track.cts.push(cts);
        track.sync.push(!(sampleFlags & SAMPLE_IS_NON_SYNC));
        dts += duration;
        chunk.length += size;
      }
      if (count > 0) track.chunks.push(chunk);
      dataEnd = offset + chunk.length;
    }
    previousEnd = dataEnd;
  }
}

/** 樣本時間改用相鄰兩個樣本的解碼時間差（tfdt 可能和累加的長度不同），並確認從 0 開始、只增不減。 */
function finishTiming(track: Track): void {
  if (track.dts.length === 0) throw new Error(`track ${track.id} has no samples`);
  if (track.dts[0] !== 0) throw new Error(`track ${track.id} starts at ${track.dts[0]}`);
  for (let i = 0; i + 1 < track.dts.length; i += 1) {
    const duration = track.dts[i + 1] - track.dts[i];
    if (duration < 0) throw new Error(`track ${track.id} decode time goes backwards`);
    track.durations[i] = duration;
  }
}

// ---- 寫出 ----

function concat(parts: Uint8Array[]): Uint8Array<ArrayBuffer> {
  const out = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
  let pos = 0;
  for (const part of parts) {
    out.set(part, pos);
    pos += part.length;
  }
  return out;
}

function makeBox(type: string, payload: Uint8Array[]): Uint8Array<ArrayBuffer> {
  const size = 8 + payload.reduce((sum, part) => sum + part.length, 0);
  if (size > UINT32_MAX) throw new Error(`${type} box too large`);
  const head = new DataView(new ArrayBuffer(8));
  head.setUint32(0, size);
  setFourcc(head, 4, type);
  return concat([new Uint8Array(head.buffer), ...payload]);
}

/** full box：version、flags 之後接 length bytes 的內容，由 fill 填入。 */
function makeFullBox(type: string, version: number, length: number, fill: (view: DataView) => void): Uint8Array<ArrayBuffer> {
  const view = new DataView(new ArrayBuffer(4 + length));
  view.setUint32(0, (version << 24) >>> 0);
  fill(new DataView(view.buffer, 4));
  return makeBox(type, [new Uint8Array(view.buffer)]);
}

function copyBox(view: DataView, box: Box): Uint8Array<ArrayBuffer> {
  const copy = new Uint8Array(box.size);
  copy.set(new Uint8Array(view.buffer, view.byteOffset + box.start, box.size));
  return copy;
}

/** 連續相同的值合併成 [次數, 值]。 */
function runs(values: number[]): [number, number][] {
  const out: [number, number][] = [];
  for (const value of values) {
    const last = out[out.length - 1];
    if (last && last[1] === value) last[0] += 1;
    else out.push([1, value]);
  }
  return out;
}

function makeFtyp(original: DataView | undefined, header: number): Uint8Array<ArrayBuffer> {
  // 主品牌改成通用的 isom（原本的 dash 表示分段檔），相容品牌保留原本的（拿掉 dash）
  const brands = ['isom'];
  if (original) {
    for (let pos = header + 8; pos + 4 <= original.byteLength; pos += 4) {
      const brand = fourcc(original, pos);
      if (brand !== 'dash' && !brands.includes(brand)) brands.push(brand);
    }
  }
  const body = new DataView(new ArrayBuffer(8 + brands.length * 4));
  setFourcc(body, 0, 'isom');
  body.setUint32(4, 0x200);
  brands.forEach((brand, i) => setFourcc(body, 8 + i * 4, brand));
  return makeBox('ftyp', [new Uint8Array(body.buffer)]);
}

/** 長度欄位是 0（分段檔常見）就填入算出的長度。 */
function patchDuration(view: DataView, box: Box, offsets: [number, number], duration: number): Uint8Array<ArrayBuffer> {
  const copy = copyBox(view, box);
  const out = new DataView(copy.buffer);
  const wide = out.getUint8(box.header) === 1;
  const at = box.header + (wide ? offsets[1] : offsets[0]);
  if (getUint(out, at, wide) !== 0) return copy;
  if (wide) out.setBigUint64(at, BigInt(duration));
  else if (duration <= UINT32_MAX) out.setUint32(at, duration);
  return copy;
}

/** 只有一段、長度是 0 的 edit list（分段檔表示「到結尾」）填入整個軌的長度。 */
function patchEditList(view: DataView, edts: Box, duration: number): Uint8Array<ArrayBuffer> {
  const parts = childrenOf(view, edts).map((box) => {
    const copy = copyBox(view, box);
    if (box.type !== 'elst') return copy;
    const out = new DataView(copy.buffer);
    const wide = out.getUint8(box.header) === 1;
    const at = box.header + 8;
    if (out.getUint32(box.header + 4) !== 1 || getUint(out, at, wide) !== 0) return copy;
    if (wide) out.setBigUint64(at, BigInt(duration));
    else if (duration <= UINT32_MAX) out.setUint32(at, duration);
    return copy;
  });
  return makeBox('edts', parts);
}

function buildMoov(
  view: DataView,
  header: number,
  movie: Movie,
  positions: (track: Track) => number[],
  wide: boolean,
): Uint8Array<ArrayBuffer> {
  const mediaDuration = (track: Track) => track.durations.reduce((sum, value) => sum + value, 0);
  const movieDuration = (track: Track) => Math.round((mediaDuration(track) * movie.timescale) / track.timescale);
  const longest = Math.max(...[...movie.tracks.values()].map(movieDuration));
  const parts: Uint8Array[] = [];
  for (const box of children(view, header, view.byteLength)) {
    if (box.type === 'mvex') continue;
    if (box.type === 'mvhd') parts.push(patchDuration(view, box, [16, 24], longest));
    else if (box.type === 'trak') parts.push(buildTrak(view, box, movie, positions, wide, mediaDuration, movieDuration));
    else parts.push(copyBox(view, box));
  }
  return makeBox('moov', parts);
}

function buildTrak(
  view: DataView,
  trak: Box,
  movie: Movie,
  positions: (track: Track) => number[],
  wide: boolean,
  mediaDuration: (track: Track) => number,
  movieDuration: (track: Track) => number,
): Uint8Array<ArrayBuffer> {
  const boxes = childrenOf(view, trak);
  const tkhd = find(boxes, 'tkhd')!;
  const tkhdBody = tkhd.start + tkhd.header;
  const track = movie.tracks.get(view.getUint32(tkhdBody + (view.getUint8(tkhdBody) === 1 ? 20 : 12)))!;
  const parts = boxes.map((box) => {
    if (box.type === 'tkhd') return patchDuration(view, box, [20, 28], movieDuration(track));
    if (box.type === 'edts') return patchEditList(view, box, movieDuration(track));
    if (box.type !== 'mdia') return copyBox(view, box);
    const mdia = childrenOf(view, box).map((child) => {
      if (child.type === 'mdhd') return patchDuration(view, child, [16, 24], mediaDuration(track));
      if (child.type !== 'minf') return copyBox(view, child);
      const minf = childrenOf(view, child).map((part) =>
        part.type === 'stbl' ? buildStbl(view, part, track, positions(track), wide) : copyBox(view, part),
      );
      return makeBox('minf', minf);
    });
    return makeBox('mdia', mdia);
  });
  return makeBox('trak', parts);
}

/** stsd 照抄，其餘換成依樣本算出的表（原本的都是空的；sgpd 等分段用的資訊不再需要）。 */
function buildStbl(view: DataView, stbl: Box, track: Track, chunkPositions: number[], wide: boolean): Uint8Array<ArrayBuffer> {
  const stsd = find(childrenOf(view, stbl), 'stsd');
  if (!stsd) throw new Error('stbl without stsd');
  const count = track.sizes.length;
  const parts: Uint8Array[] = [copyBox(view, stsd)];

  const durationRuns = runs(track.durations);
  parts.push(
    makeFullBox('stts', 0, 4 + durationRuns.length * 8, (body) => {
      body.setUint32(0, durationRuns.length);
      durationRuns.forEach(([times, value], i) => {
        body.setUint32(4 + i * 8, times);
        body.setUint32(8 + i * 8, value);
      });
    }),
  );

  if (track.cts.some((value) => value !== 0)) {
    const signed = track.cts.some((value) => value < 0);
    const ctsRuns = runs(track.cts);
    parts.push(
      makeFullBox('ctts', signed ? 1 : 0, 4 + ctsRuns.length * 8, (body) => {
        body.setUint32(0, ctsRuns.length);
        ctsRuns.forEach(([times, value], i) => {
          body.setUint32(4 + i * 8, times);
          if (signed) body.setInt32(8 + i * 8, value);
          else body.setUint32(8 + i * 8, value);
        });
      }),
    );
  }

  // 全部都是關鍵格時省略 stss（有 stss 但沒有內容代表「沒有關鍵格」）
  if (track.sync.some((sync) => !sync)) {
    const keys = track.sync.flatMap((sync, i) => (sync ? [i + 1] : []));
    parts.push(
      makeFullBox('stss', 0, 4 + keys.length * 4, (body) => {
        body.setUint32(0, keys.length);
        keys.forEach((number, i) => body.setUint32(4 + i * 4, number));
      }),
    );
  }

  const uniform = track.sizes.every((size) => size === track.sizes[0]);
  parts.push(
    makeFullBox('stsz', 0, 8 + (uniform ? 0 : count * 4), (body) => {
      body.setUint32(0, uniform ? track.sizes[0] : 0);
      body.setUint32(4, count);
      if (!uniform) track.sizes.forEach((size, i) => body.setUint32(8 + i * 4, size));
    }),
  );

  const groups: [number, number, number][] = [];
  track.chunks.forEach((chunk, i) => {
    const last = groups[groups.length - 1];
    if (!last || last[1] !== chunk.samples || last[2] !== chunk.description) groups.push([i + 1, chunk.samples, chunk.description]);
  });
  parts.push(
    makeFullBox('stsc', 0, 4 + groups.length * 12, (body) => {
      body.setUint32(0, groups.length);
      groups.forEach(([first, samples, description], i) => {
        body.setUint32(4 + i * 12, first);
        body.setUint32(8 + i * 12, samples);
        body.setUint32(12 + i * 12, description);
      });
    }),
  );

  parts.push(
    makeFullBox(wide ? 'co64' : 'stco', 0, 4 + chunkPositions.length * (wide ? 8 : 4), (body) => {
      body.setUint32(0, chunkPositions.length);
      chunkPositions.forEach((position, i) => {
        if (wide) body.setBigUint64(4 + i * 8, BigInt(position));
        else body.setUint32(4 + i * 4, position);
      });
    }),
  );
  return makeBox('stbl', parts);
}
