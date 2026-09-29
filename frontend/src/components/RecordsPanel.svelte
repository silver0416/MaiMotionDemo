<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import ContextMenu, { type MenuItem } from './ContextMenu.svelte';
  import Popover from './Popover.svelte';
  import {
    TAG_COLORS,
    cleanName,
    recordDate,
    recordLabeled,
    recordSearchText,
    recordTitle,
    recordTopLevel,
    records,
    type ChartRecord,
    type RecordFolder,
    type RecordTag,
  } from '../state/records.svelte';
  import { exportRecords } from '../state/recordsIO.svelte';
  import { saveExport, showSaved } from '../state/exportPrefs.svelte';
  import { errorLog } from '../state/errorLog.svelte';
  import { toasts } from '../state/toasts.svelte';
  import { squash } from '../lib/press';
  import { session } from '../state/session.svelte';

  interface Props {
    onCreate: () => void;
    onSearch: () => void;
    onCollapse: () => void;
    onEdit: (record: ChartRecord) => void;
    onSettings: () => void;
  }

  let { onCreate, onSearch, onCollapse, onEdit, onSettings }: Props = $props();

  // ---- 檢視：資料夾、標籤篩選、文字篩選、排序（本機記住） ----

  type SortKey = 'created-desc' | 'created-asc' | 'opened' | 'name' | 'level' | 'labeled';
  const SORTS: { id: SortKey; label: string }[] = [
    { id: 'created-desc', label: '最新新增' },
    { id: 'created-asc', label: '最早新增' },
    { id: 'opened', label: '最近開啟' },
    { id: 'name', label: '名稱' },
    { id: 'level', label: '難度高到低' },
    { id: 'labeled', label: '標註最多' },
  ];
  /** 'all' 全部、'none' 未分類，其餘為資料夾 id。 */
  type FolderView = string;
  const VIEW_KEY = 'maimotion.records.view';

  function readView(): { folder: FolderView; sort: SortKey; tags: string[] } {
    try {
      const raw = JSON.parse(localStorage.getItem(VIEW_KEY) ?? '{}') as Record<string, unknown>;
      return {
        folder: typeof raw.folder === 'string' ? raw.folder : 'all',
        sort: SORTS.some((item) => item.id === raw.sort) ? (raw.sort as SortKey) : 'created-desc',
        tags: Array.isArray(raw.tags) ? raw.tags.filter((tag): tag is string => typeof tag === 'string') : [],
      };
    } catch {
      return { folder: 'all', sort: 'created-desc', tags: [] };
    }
  }

  const initial = readView();
  let folderView = $state<FolderView>(initial.folder);
  let sortKey = $state<SortKey>(initial.sort);
  let tagFilter = $state<string[]>(initial.tags);
  let query = $state('');

  $effect(() => {
    const value = JSON.stringify({ folder: folderView, sort: sortKey, tags: tagFilter });
    try {
      localStorage.setItem(VIEW_KEY, value);
    } catch {
      // 只是方便下次開啟，寫不進去不影響使用。
    }
  });

  // 資料夾或標籤被刪掉、改名後，檢視回到仍存在的項目。
  $effect(() => {
    if (!records.loaded) return;
    if (folderView !== 'all' && folderView !== 'none' && !records.folderById(folderView)) folderView = 'all';
    const known = tagFilter.filter((tag) => records.tags.some((item) => item.name === tag));
    if (known.length !== tagFilter.length) tagFilter = known;
  });

  const folderCounts = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const record of records.items) {
      const key = record.folder ?? 'none';
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    return counts;
  });

  const tagCounts = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const record of records.items) for (const tag of record.tags ?? []) counts.set(tag, (counts.get(tag) ?? 0) + 1);
    return counts;
  });

  const collator = new Intl.Collator('zh-Hant', { numeric: true, sensitivity: 'base' });

  const visible = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    const list = records.items.filter((record) => {
      if (folderView === 'none' ? record.folder !== undefined : folderView !== 'all' && record.folder !== folderView) {
        return false;
      }
      if (tagFilter.length > 0 && !tagFilter.some((tag) => record.tags?.includes(tag))) return false;
      if (needle) {
        const text = `${recordSearchText(record)}\n${recordTitle(record).toLowerCase()}\n${(record.tags ?? []).join('\n').toLowerCase()}`;
        if (!text.includes(needle)) return false;
      }
      return true;
    });
    const byCreated = (a: ChartRecord, b: ChartRecord) => b.createdAt - a.createdAt;
    const compare: Record<SortKey, (a: ChartRecord, b: ChartRecord) => number> = {
      'created-desc': byCreated,
      'created-asc': (a, b) => a.createdAt - b.createdAt,
      opened: (a, b) => (b.openedAt ?? 0) - (a.openedAt ?? 0) || byCreated(a, b),
      name: (a, b) => collator.compare(recordTitle(a), recordTitle(b)) || byCreated(a, b),
      level: (a, b) => recordTopLevel(b) - recordTopLevel(a) || byCreated(a, b),
      labeled: (a, b) => recordLabeled(b) - recordLabeled(a) || byCreated(a, b),
    };
    return list.sort(compare[sortKey]);
  });

  const filtering = $derived(query.trim() !== '' || tagFilter.length > 0);

  function toggleTagFilter(name: string) {
    tagFilter = tagFilter.includes(name) ? tagFilter.filter((tag) => tag !== name) : [...tagFilter, name];
  }

  // ---- 多選 ----

  let selected = $state<string[]>([]);
  let anchor: string | null = null;
  const selectedSet = $derived(new Set(selected));
  /**
   * 選取模式：右鍵「選取」、Ctrl／Shift 點選或 Ctrl+A 進入；此時點一下就加入或移出選取，
   * 不開啟譜面。選取的外觀與選取工具列都只在這個模式出現，一筆也一樣。
   */
  let selecting = $state(false);
  const multi = $derived(selecting);

  // 被刪掉或不在清單裡的紀錄不留在選取中。
  $effect(() => {
    const ids = new Set(records.items.map((record) => record.id));
    if (selected.some((id) => !ids.has(id))) selected = selected.filter((id) => ids.has(id));
  });

  // 選取清空（移出最後一筆、刪除選取的紀錄等）就結束選取模式，不會留下「已選 0 筆」。
  $effect(() => {
    if (selecting && selected.length === 0) clearSelection();
  });

  /** 工具列顯示的筆數；收起動畫期間維持最後的數字，不會閃成 0。 */
  let shownCount = $state(0);
  $effect(() => {
    if (selecting && selected.length > 0) shownCount = selected.length;
  });

  function selectOnly(id: string) {
    selected = [id];
    anchor = id;
  }

  /** 加入或移出選取；全部移出時結束選取模式。 */
  function toggleSelect(id: string) {
    selected = selectedSet.has(id) ? selected.filter((item) => item !== id) : [...selected, id];
    anchor = id;
    selecting = true;
  }

  function selectRange(id: string, additive: boolean) {
    const order = visible.map((record) => record.id);
    const from = anchor ? order.indexOf(anchor) : -1;
    const to = order.indexOf(id);
    if (from < 0 || to < 0) {
      selectOnly(id);
      return;
    }
    const range = order.slice(Math.min(from, to), Math.max(from, to) + 1);
    selected = additive ? [...new Set([...selected, ...range])] : range;
    selecting = true;
  }

  function selectAllVisible() {
    selected = visible.map((record) => record.id);
    anchor = selected[0] ?? null;
    if (selected.length > 0) selecting = true;
  }

  function clearSelection() {
    selecting = false;
    selected = records.activeId ? [records.activeId] : [];
    confirmingBulk = false;
  }

  /** 右鍵「選取」：進入選取模式並加入這筆。 */
  function startSelecting(id: string) {
    if (!multi) selected = [];
    selecting = true;
    if (!selected.includes(id)) selected = [...selected, id];
    anchor = id;
  }

  /** 依目前清單順序排列的選取。 */
  function orderedSelection(): string[] {
    const order = visible.map((record) => record.id);
    return [...selected].sort((a, b) => {
      const ia = order.indexOf(a);
      const ib = order.indexOf(b);
      return (ia < 0 ? Infinity : ia) - (ib < 0 ? Infinity : ib);
    });
  }

  function onRowClick(record: ChartRecord, event: MouseEvent) {
    confirmingId = null;
    if (event.shiftKey) {
      selectRange(record.id, event.ctrlKey || event.metaKey);
      return;
    }
    if (event.ctrlKey || event.metaKey || selecting) {
      // 剛進入選取模式時從空的開始，目前開啟的譜面不會自動算在內。
      if (!selecting) selected = [];
      toggleSelect(record.id);
      return;
    }
    selectOnly(record.id);
    void open(record);
  }

  function onListKeydown(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'a') {
      event.preventDefault();
      selectAllVisible();
    } else if (event.key === 'Escape' && multi) {
      event.preventDefault();
      clearSelection();
    }
  }

  // ---- 右鍵選單 ----

  /** 三點按鈕與右鍵共用同一個選單；fromButton 決定關閉後焦點回到哪裡。 */
  let menu = $state<{ id: string; x: number; y: number; align: 'start' | 'end'; fromButton: boolean } | null>(null);
  const menuRecord = $derived(menu ? records.get(menu.id) : null);
  const menuBulk = $derived(menu !== null && multi && selectedSet.has(menu.id));

  const menuItems = $derived.by((): MenuItem[] => {
    if (menuBulk) {
      const n = selected.length;
      return [
        { id: 'export', label: `匯出 ${n} 份標記檔`, icon: 'download' },
        { id: 'tag', label: `標籤（${n} 筆）…`, icon: 'tag' },
        { id: 'folder', label: `移到資料夾（${n} 筆）…`, icon: 'folder' },
        { id: 'unselect', label: '取消選取這筆', icon: 'circle-dot', divider: true },
        { id: 'select-all', label: '全選目前清單', icon: 'list-checks' },
        { id: 'clear', label: '結束選取', icon: 'x' },
        { id: 'delete', label: `刪除 ${n} 筆`, icon: 'trash', danger: true, divider: true },
      ];
    }
    return [
      { id: 'edit', label: '查看／編輯', icon: 'pencil' },
      { id: 'export', label: '匯出標記檔', icon: 'download' },
      { id: 'tag', label: '標籤…', icon: 'tag' },
      { id: 'folder', label: '移到資料夾…', icon: 'folder' },
      { id: 'select', label: selecting ? '加入選取' : '選取', icon: 'circle-check', divider: true },
      { id: 'select-all', label: '全選目前清單', icon: 'list-checks' },
      { id: 'delete', label: '刪除', icon: 'trash', danger: true, divider: true },
    ];
  });

  function openMenuFromButton(record: ChartRecord, event: MouseEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    if (menu?.id === record.id && menu.fromButton) {
      menu = null;
      return;
    }
    confirmingId = null;
    if (!selectedSet.has(record.id) && !selecting) selectOnly(record.id);
    menu = { id: record.id, x: rect.right, y: rect.bottom + 4, align: 'end', fromButton: true };
  }

  function openContextMenu(record: ChartRecord, event: MouseEvent) {
    event.preventDefault();
    confirmingId = null;
    if (!selectedSet.has(record.id) && !selecting) selectOnly(record.id);
    menu = { id: record.id, x: event.clientX, y: event.clientY, align: 'start', fromButton: false };
  }

  async function closeMenu(focusBack: boolean) {
    const current = menu;
    menu = null;
    if (!focusBack || !current || !listEl) return;
    await tick();
    const row = listEl.querySelector(`[data-record-id="${CSS.escape(current.id)}"]`);
    row?.querySelector<HTMLElement>(current.fromButton ? '.more' : '.open')?.focus();
  }

  async function selectMenu(action: string) {
    const current = menu;
    const record = menuRecord;
    const bulk = menuBulk;
    menu = null;
    if (!record || !current) return;
    const ids = bulk ? orderedSelection() : [record.id];
    const at = { x: current.x, y: current.y, align: current.align };
    switch (action) {
      case 'edit':
        onEdit(record);
        break;
      case 'export':
        await exportIds(ids);
        break;
      case 'tag':
        picker = { kind: 'tag', ids, ...at };
        break;
      case 'folder':
        picker = { kind: 'folder', ids, ...at };
        break;
      case 'select':
        startSelecting(record.id);
        break;
      case 'unselect':
        toggleSelect(record.id);
        break;
      case 'select-all':
        selecting = true;
        selectAllVisible();
        break;
      case 'clear':
        clearSelection();
        break;
      case 'delete':
        if (bulk) {
          confirmingBulk = true;
        } else {
          confirmingId = record.id;
          await tick();
          // 焦點先放在「取消」，誤按 Enter 不會直接刪掉。
          listEl
            ?.querySelector<HTMLElement>(`[data-record-id="${CSS.escape(record.id)}"] .confirm .btn:not(.btn--danger)`)
            ?.focus();
        }
        break;
    }
  }

  /** 列上按 Shift+F10 或選單鍵，和右鍵一樣打開選單。 */
  function onRowKeydown(record: ChartRecord, event: KeyboardEvent) {
    if (event.key === 'ContextMenu' || (event.key === 'F10' && event.shiftKey)) {
      event.preventDefault();
      const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
      confirmingId = null;
      if (!selectedSet.has(record.id) && !selecting) selectOnly(record.id);
      menu = { id: record.id, x: rect.left + 24, y: rect.bottom, align: 'start', fromButton: false };
    } else if (event.key === ' ' && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      toggleSelect(record.id);
    }
  }

  // ---- 資料夾／標籤的選擇與管理（浮動面板） ----

  type Picker =
    | { kind: 'tag' | 'folder'; ids: string[] }
    | { kind: 'new-folder' }
    | { kind: 'rename-folder'; folder: RecordFolder }
    | { kind: 'rename-tag'; tag: RecordTag }
    | { kind: 'tag-color'; tag: RecordTag };
  let picker = $state<(Picker & { x: number; y: number; align: 'start' | 'end' }) | null>(null);
  let pickerText = $state('');
  let pickerError = $state('');

  $effect(() => {
    if (!picker) return;
    pickerText = picker.kind === 'rename-folder' ? picker.folder.name : picker.kind === 'rename-tag' ? picker.tag.name : '';
    pickerError = '';
  });

  function closePicker() {
    picker = null;
  }

  function tagState(ids: string[], name: string): 'all' | 'some' | 'none' {
    const count = ids.filter((id) => records.get(id)?.tags?.includes(name)).length;
    return count === 0 ? 'none' : count === ids.length ? 'all' : 'some';
  }

  function commonFolder(ids: string[]): string | null | undefined {
    const folders = new Set(ids.map((id) => records.get(id)?.folder ?? null));
    return folders.size === 1 ? [...folders][0] : undefined;
  }

  function toggleTag(ids: string[], name: string) {
    records.setTag(ids, name, tagState(ids, name) !== 'all');
  }

  function submitPicker(event: SubmitEvent) {
    event.preventDefault();
    if (!picker) return;
    const name = cleanName(pickerText);
    if (!name) {
      pickerError = '請輸入名稱。';
      return;
    }
    switch (picker.kind) {
      case 'tag':
        records.setTag(picker.ids, name, true);
        pickerText = '';
        break;
      case 'folder': {
        const folder = records.createFolder(name);
        if (folder) moveIds(picker.ids, folder);
        closePicker();
        break;
      }
      case 'new-folder': {
        const folder = records.createFolder(name);
        if (folder) folderView = folder.id;
        closePicker();
        break;
      }
      case 'rename-folder':
        if (!records.renameFolder(picker.folder.id, name)) {
          pickerError = '已經有同名的資料夾。';
          return;
        }
        closePicker();
        break;
      case 'rename-tag': {
        const from = picker.tag.name;
        records.renameTag(from, name);
        tagFilter = tagFilter.map((tag) => (tag === from ? name : tag));
        closePicker();
        break;
      }
    }
  }

  function moveIds(ids: string[], folder: RecordFolder | null) {
    records.moveTo(ids, folder?.id ?? null);
    toasts.show({
      id: 'records-move',
      tone: 'ok',
      title: folder ? `已移到「${folder.name}」` : '已移到未分類',
      body: `${ids.length} 筆紀錄`,
    });
  }

  function pickerAt(event: MouseEvent, align: 'start' | 'end' = 'start') {
    if (event.type === 'contextmenu') return { x: event.clientX, y: event.clientY, align };
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    return { x: align === 'end' ? rect.right : rect.left, y: rect.bottom + 4, align };
  }

  // 資料夾與標籤的右鍵選單（和紀錄的選單分開）。
  let chipMenu = $state<
    | { kind: 'folder'; folder: RecordFolder; x: number; y: number }
    | { kind: 'tag'; tag: RecordTag; x: number; y: number }
    | null
  >(null);

  const chipItems = $derived.by((): MenuItem[] => {
    if (!chipMenu) return [];
    if (chipMenu.kind === 'folder') {
      return [
        { id: 'rename', label: '重新命名', icon: 'pencil' },
        { id: 'export', label: '匯出資料夾內的標記檔', icon: 'download' },
        { id: 'delete', label: '刪除資料夾', icon: 'trash', danger: true, divider: true },
      ];
    }
    return [
      { id: 'rename', label: '重新命名', icon: 'pencil' },
      { id: 'color', label: '換顏色', icon: 'circle-dot' },
      { id: 'delete', label: '刪除標籤', icon: 'trash', danger: true, divider: true },
    ];
  });

  function openChipMenu(target: { kind: 'folder'; folder: RecordFolder } | { kind: 'tag'; tag: RecordTag }, event: MouseEvent) {
    event.preventDefault();
    chipMenu = { ...target, x: event.clientX, y: event.clientY } as typeof chipMenu;
  }

  async function selectChipMenu(action: string) {
    const current = chipMenu;
    chipMenu = null;
    if (!current) return;
    const at = { x: current.x, y: current.y, align: 'start' as const };
    if (current.kind === 'folder') {
      const { folder } = current;
      if (action === 'rename') picker = { kind: 'rename-folder', folder, ...at };
      if (action === 'export') {
        await exportIds(visibleIn(folder.id));
      }
      if (action === 'delete') {
        const moved = records.deleteFolder(folder.id);
        toasts.show({
          id: 'records-folder-delete',
          tone: 'info',
          title: `已刪除資料夾「${folder.name}」`,
          body: moved.length > 0 ? `${moved.length} 筆紀錄改到未分類，紀錄本身沒有刪除。` : undefined,
          action: { label: '復原', icon: 'rotate-ccw', run: () => records.restoreFolder(folder, moved) },
        });
      }
    } else {
      const { tag } = current;
      if (action === 'rename') picker = { kind: 'rename-tag', tag, ...at };
      if (action === 'color') picker = { kind: 'tag-color', tag, ...at };
      if (action === 'delete') {
        const tagged = records.deleteTag(tag.name);
        toasts.show({
          id: 'records-tag-delete',
          tone: 'info',
          title: `已刪除標籤「${tag.name}」`,
          body: tagged.length > 0 ? `從 ${tagged.length} 筆紀錄拿掉。` : undefined,
          action: {
            label: '復原',
            icon: 'rotate-ccw',
            run: () => {
              records.createTag(tag.name, tag.color);
              records.setTag(tagged, tag.name, true);
            },
          },
        });
      }
    }
  }

  /** 資料夾內的紀錄，依目前的排序。 */
  function visibleIn(folderId: string): string[] {
    const inFolder = records.items.filter((record) => record.folder === folderId).map((record) => record.id);
    const order = new Map(visible.map((record, index) => [record.id, index]));
    return inFolder.sort((a, b) => (order.get(a) ?? Infinity) - (order.get(b) ?? Infinity));
  }

  // ---- 拖曳紀錄到資料夾或標籤 ----

  const DRAG_TYPE = 'application/x-maimotion-records';
  let dropTarget = $state<string | null>(null);

  function onDragStart(record: ChartRecord, event: DragEvent) {
    const ids = selectedSet.has(record.id) ? orderedSelection() : [record.id];
    event.dataTransfer?.setData(DRAG_TYPE, JSON.stringify(ids));
    event.dataTransfer?.setData('text/plain', ids.map((id) => recordTitle(records.get(id)!)).join('\n'));
    if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move';
  }

  function dragIds(event: DragEvent): string[] | null {
    try {
      const raw = event.dataTransfer?.getData(DRAG_TYPE);
      const ids = raw ? (JSON.parse(raw) as unknown) : null;
      return Array.isArray(ids) ? ids.filter((id): id is string => typeof id === 'string') : null;
    } catch {
      return null;
    }
  }

  function onDragOver(key: string, event: DragEvent) {
    if (!event.dataTransfer?.types.includes(DRAG_TYPE)) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
    dropTarget = key;
  }

  function onDropFolder(folder: RecordFolder | null, event: DragEvent) {
    dropTarget = null;
    const ids = dragIds(event);
    if (!ids) return;
    event.preventDefault();
    moveIds(ids, folder);
  }

  function onDropTag(tag: RecordTag, event: DragEvent) {
    dropTarget = null;
    const ids = dragIds(event);
    if (!ids) return;
    event.preventDefault();
    records.setTag(ids, tag.name, true);
    toasts.show({ id: 'records-tag', tone: 'ok', title: `已加上標籤「${tag.name}」`, body: `${ids.length} 筆紀錄` });
  }

  // ---- 匯出／匯入 ----

  let exporting = $state(false);

  async function exportIds(ids: string[]) {
    if (ids.length === 0 || exporting) return;
    exporting = true;
    try {
      const result = await exportRecords(ids);
      if (!result) return;
      const path = await saveExport(result.text, result.fileName);
      if (!path) return;
      const notes = [];
      if (result.unlabeled > 0) notes.push(`${result.unlabeled} 份還沒有手順（仍附原譜與整理資訊）`);
      if (result.unordered > 0) notes.push(`${result.unordered} 份沒有解析結果，音符依時間排列`);
      showSaved('records-export', `已匯出 ${result.count} 份標記檔`, path, notes.join('；') || undefined);
    } catch (error) {
      toasts.show({ id: 'records-export', tone: 'error', title: '匯出失敗', body: String(error) });
    } finally {
      exporting = false;
    }
  }

  // ---- 刪除 ----

  /** 單筆刪除要按兩次：第一次只把該列切成確認狀態。 */
  let confirmingId = $state<string | null>(null);
  let confirmingBulk = $state(false);

  function remove(record: ChartRecord) {
    const wasActive = records.activeId === record.id;
    const removed = records.removeMany([record.id]);
    confirmingId = null;
    if (wasActive) session.clearResult();
    offerRestore(removed);
  }

  function removeSelected() {
    const ids = orderedSelection();
    const wasActive = records.activeId !== null && ids.includes(records.activeId);
    const removed = records.removeMany(ids);
    confirmingBulk = false;
    selected = [];
    if (wasActive) session.clearResult();
    offerRestore(removed);
  }

  function offerRestore(removed: ChartRecord[]) {
    if (removed.length === 0) return;
    toasts.show({
      id: 'records-delete',
      tone: 'info',
      title: removed.length === 1 ? `已刪除「${recordTitle(removed[0])}」` : `已刪除 ${removed.length} 筆紀錄`,
      body: '標註與時間軸標籤一起刪除；按「復原」可以放回來。',
      action: { label: '復原', icon: 'rotate-ccw', run: () => records.restore(removed) },
    });
  }

  // ---- 開啟 ----

  const analyzing = $derived(session.phase === 'analyzing');

  let listEl: HTMLElement | null = $state(null);

  // 從搜尋視窗跳到既有紀錄時，把那一列捲進可視範圍並選取。
  $effect(() => {
    const id = records.activeId;
    if (!id) return;
    if (!multi && selected[0] !== id) selectOnly(id);
    listEl?.querySelector(`[data-record-id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: 'nearest' });
  });

  async function open(record: ChartRecord) {
    if (analyzing || !session.desktop) return;
    records.activeId = record.id;
    await session.load(record.source);
  }

  const folderName = (id: string | undefined) => records.folderById(id)?.name;
</script>

<div class="records">
  <div class="records-head">
    <button id="records-collapse" class="btn btn--ghost btn--wide" use:squash onclick={onCollapse}>
      <span class="lead"><Icon name="chevron-left" size={16} /></span>收合側欄
    </button>
    <button class="btn btn--ghost btn--wide" onclick={onCreate}>
      <span class="lead lead--disc"><Icon name="plus" size={14} /></span>新增
    </button>
    <button class="btn btn--ghost btn--wide" onclick={onSearch}>
      <span class="lead lead--disc"><Icon name="search" size={14} /></span>搜尋譜面
    </button>
  </div>

  <div class="swap swap--title" class:is-shut={multi} inert={multi}>
    <div class="records-title">
      <span>譜面紀錄</span>
      <span class="mono">{filtering || folderView !== 'all' ? `${visible.length} / ${records.items.length}` : records.items.length}</span>
    </div>
  </div>

  <div class="tools">
    <div class="chips" role="group" aria-label="資料夾">
      <button
        class="chip"
        class:is-on={folderView === 'all'}
        aria-pressed={folderView === 'all'}
        onclick={() => (folderView = 'all')}
      >
        全部<span class="chip-count">{records.items.length}</span>
      </button>
      <button
        class="chip"
        class:is-on={folderView === 'none'}
        class:is-drop={dropTarget === 'folder:none'}
        aria-pressed={folderView === 'none'}
        onclick={() => (folderView = 'none')}
        ondragover={(event) => onDragOver('folder:none', event)}
        ondragleave={() => (dropTarget = null)}
        ondrop={(event) => onDropFolder(null, event)}
        title="沒有放進資料夾的紀錄；把紀錄拖到這裡移出資料夾"
      >
        未分類<span class="chip-count">{folderCounts.get('none') ?? 0}</span>
      </button>
      {#each records.folders as folder (folder.id)}
        <button
          class="chip"
          class:is-on={folderView === folder.id}
          class:is-drop={dropTarget === `folder:${folder.id}`}
          aria-pressed={folderView === folder.id}
          onclick={() => (folderView = folder.id)}
          oncontextmenu={(event) => openChipMenu({ kind: 'folder', folder }, event)}
          ondragover={(event) => onDragOver(`folder:${folder.id}`, event)}
          ondragleave={() => (dropTarget = null)}
          ondrop={(event) => onDropFolder(folder, event)}
          title="右鍵：重新命名、匯出或刪除；可以把紀錄拖進來"
        >
          <Icon name="folder" size={12} />{folder.name}<span class="chip-count">{folderCounts.get(folder.id) ?? 0}</span>
        </button>
      {/each}
      <button
        class="chip chip--icon"
        onclick={(event) => (picker = { kind: 'new-folder', ...pickerAt(event) })}
        aria-label="新增資料夾"
        title="新增資料夾"
      >
        <Icon name="folder-plus" size={13} />
      </button>
    </div>

    {#if records.tags.length > 0}
      <div class="chips" role="group" aria-label="依標籤篩選（符合任一）">
        {#each records.tags as tag (tag.name)}
          <button
            class="chip"
            class:is-on={tagFilter.includes(tag.name)}
            class:is-drop={dropTarget === `tag:${tag.name}`}
            aria-pressed={tagFilter.includes(tag.name)}
            onclick={() => toggleTagFilter(tag.name)}
            oncontextmenu={(event) => openChipMenu({ kind: 'tag', tag }, event)}
            ondragover={(event) => onDragOver(`tag:${tag.name}`, event)}
            ondragleave={() => (dropTarget = null)}
            ondrop={(event) => onDropTag(tag, event)}
            title="點選篩選（符合任一）；右鍵改名、換色或刪除；可以把紀錄拖到這裡加上標籤"
          >
            <span class="dot" style={`background:${tag.color}`}></span>{tag.name}<span class="chip-count"
              >{tagCounts.get(tag.name) ?? 0}</span
            >
          </button>
        {/each}
      </div>
    {/if}

    <div class="filter-row">
      <label class="filter">
        <span class="sr-only">篩選譜面紀錄</span>
        <Icon name="search" size={13} />
        <input class="filter-input" type="search" placeholder="篩選名稱、曲師、標籤" bind:value={query} />
        {#if filtering}
          <button
            class="filter-clear"
            onclick={(event) => {
              event.preventDefault();
              query = '';
              tagFilter = [];
            }}
            aria-label="清除篩選"
            title="清除文字與標籤篩選"
          >
            <Icon name="x" size={12} />
          </button>
        {/if}
      </label>
      <label class="sort" title="排列方式">
        <span class="sr-only">排列方式</span>
        <Icon name="arrow-up-down" size={13} />
        <select class="sort-select" bind:value={sortKey}>
          {#each SORTS as sort (sort.id)}
            <option value={sort.id}>{sort.label}</option>
          {/each}
        </select>
      </label>
    </div>
  </div>

  <div class="swap swap--bar" class:is-shut={!multi} inert={!multi}>
    <div class="selection" role="toolbar" aria-label="選取的紀錄">
      {#if confirmingBulk}
        <span class="small">刪除 {selected.length} 筆紀錄？</span>
        <span class="spacer"></span>
        <button class="btn btn--danger" onclick={removeSelected}><Icon name="trash" />刪除</button>
        <button class="btn" onclick={() => (confirmingBulk = false)}><Icon name="x" />取消</button>
      {:else}
        <span class="small">已選 <strong>{shownCount}</strong> 筆</span>
        <span class="spacer"></span>
        <button
          class="btn btn--icon"
          onclick={() => exportIds(orderedSelection())}
          disabled={exporting || selected.length === 0}
          aria-label="匯出標記檔"
          title="匯出標記檔（一個檔案）"
        >
          <Icon name={exporting ? 'loader' : 'download'} spin={exporting} size={15} />
        </button>
        <button
          class="btn btn--icon"
          onclick={(event) => (picker = { kind: 'tag', ids: orderedSelection(), ...pickerAt(event, 'end') })}
          disabled={selected.length === 0}
          aria-label="標籤"
          title="加上或拿掉標籤"
        >
          <Icon name="tag" size={15} />
        </button>
        <button
          class="btn btn--icon"
          onclick={(event) => (picker = { kind: 'folder', ids: orderedSelection(), ...pickerAt(event, 'end') })}
          disabled={selected.length === 0}
          aria-label="移到資料夾"
          title="移到資料夾"
        >
          <Icon name="folder" size={15} />
        </button>
        <button
          class="btn btn--icon"
          onclick={() => (confirmingBulk = true)}
          disabled={selected.length === 0}
          aria-label="刪除"
          title="刪除"
        >
          <Icon name="trash" size={15} />
        </button>
        <button class="btn btn--icon" onclick={clearSelection} aria-label="結束選取" title="結束選取（Esc）">
          <Icon name="x" size={15} />
        </button>
      {/if}
    </div>
  </div>

  {#if !session.desktop}
    <p class="records-note">
      <span class="badge badge--sample">瀏覽器預覽</span>
      <span class="field-hint">沒有 Rust 核心，只能瀏覽介面，無法生成。</span>
    </p>
  {/if}

  <div class="records-body scroll">
    {#if !records.loaded && records.items.length === 0}
      <p class="empty small muted">讀取紀錄中…</p>
    {:else if records.items.length === 0}
      <p class="empty small muted">還沒有譜面。按「新增」貼上 simai 或 maidata.txt，或從 Majdata、simai Wiki 搜尋匯入。</p>
    {:else if visible.length === 0}
      <p class="empty small muted">
        {filtering ? '沒有符合篩選的紀錄。' : folderView === 'none' ? '所有紀錄都已放進資料夾。' : '這個資料夾還是空的。可以把紀錄拖到上方的資料夾，或在紀錄上按右鍵「移到資料夾」。'}
      </p>
    {:else}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <ul
        class="list"
        bind:this={listEl}
        onkeydown={onListKeydown}
        aria-label="譜面紀錄（Ctrl／Shift 點選可多選）"
      >
        {#each visible as record (record.id)}
          {@const active = records.activeId === record.id}
          {@const picked = multi && selectedSet.has(record.id)}
          {@const folder = folderView === 'all' ? folderName(record.folder) : undefined}
          <li
            class="item"
            class:is-active={active}
            class:is-selected={picked}
            class:is-menu={menu?.id === record.id}
            data-record-id={record.id}
            draggable={confirmingId !== record.id}
            ondragstart={(event) => onDragStart(record, event)}
            oncontextmenu={(event) => confirmingId !== record.id && openContextMenu(record, event)}
          >
            {#if confirmingId === record.id}
              <div class="confirm">
                <span class="small">刪除這筆紀錄？</span>
                <div class="row">
                  <button class="btn btn--danger" onclick={() => remove(record)}>
                    <Icon name="trash" />刪除
                  </button>
                  <button class="btn" onclick={() => (confirmingId = null)}>
                    <Icon name="x" />取消
                  </button>
                </div>
              </div>
            {:else}
              <button
                class="open"
                onkeydown={(event) => onRowKeydown(record, event)}
                onclick={(event) => onRowClick(record, event)}
                aria-current={active ? 'true' : undefined}
              >
                <span class="item-title">
                  <Icon name={picked ? 'circle-check' : 'file-text'} size={13} />
                  <span class="item-title-text">{recordTitle(record)}</span>
                  {#if picked}<span class="sr-only">（已選取）</span>{/if}
                </span>
                <span class="item-preview xsmall mono">
                  {recordDate(record)}
                  {#if folder}<span class="item-folder"><Icon name="folder" size={11} />{folder}</span>{/if}
                </span>
                {#if record.tags && record.tags.length > 0}
                  <span class="item-tags xsmall">
                    {#each record.tags as tag (tag)}
                      <span class="item-tag"><span class="dot" style={`background:${records.tagColor(tag)}`}></span>{tag}</span>
                    {/each}
                  </span>
                {/if}
              </button>
              <button
                class="more btn btn--icon"
                onclick={(event) => openMenuFromButton(record, event)}
                aria-label={`${recordTitle(record)} 的更多動作`}
                aria-haspopup="menu"
                aria-expanded={menu?.id === record.id}
                title="更多動作（也可以在列上按右鍵）"
              >
                <Icon name="more-horizontal" size={16} />
              </button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <div class="records-foot">
    <button class="btn btn--ghost btn--wide" onclick={onSettings}>
      <span class="lead"><Icon name="settings" size={16} /></span>設定
      {#if errorLog.entries.length > 0}
        <span class="spacer"></span>
        <span class="badge badge--danger" title="有尚未回報的錯誤紀錄">
          {errorLog.entries.length}<span class="sr-only"> 筆錯誤紀錄</span>
        </span>
      {/if}
    </button>
  </div>
</div>

{#if menu && menuRecord}
  <ContextMenu
    items={menuItems}
    x={menu.x}
    y={menu.y}
    align={menu.align}
    label={menuBulk ? `選取的 ${selected.length} 筆紀錄的動作` : `${recordTitle(menuRecord)} 的動作`}
    onSelect={selectMenu}
    onClose={closeMenu}
  />
{/if}

{#if chipMenu}
  <ContextMenu
    items={chipItems}
    x={chipMenu.x}
    y={chipMenu.y}
    label={chipMenu.kind === 'folder' ? `資料夾「${chipMenu.folder.name}」的動作` : `標籤「${chipMenu.tag.name}」的動作`}
    onSelect={selectChipMenu}
    onClose={() => (chipMenu = null)}
  />
{/if}

{#if picker}
  {@const p = picker}
  <Popover
    x={p.x}
    y={p.y}
    align={p.align}
    width={p.kind === 'tag-color' ? 196 : 248}
    label={p.kind === 'tag' ? '標籤' : p.kind === 'folder' ? '移到資料夾' : p.kind === 'tag-color' ? '標籤顏色' : '名稱'}
    onClose={closePicker}
  >
    {#if p.kind === 'tag'}
      <div class="pick-head xsmall muted">{p.ids.length > 1 ? `${p.ids.length} 筆紀錄的標籤` : '標籤'}</div>
      {#if records.tags.length > 0}
        <div class="pick-list scroll" role="group" aria-label="標籤">
          {#each records.tags as tag (tag.name)}
            {@const state = tagState(p.ids, tag.name)}
            <button
              class="pick-item"
              role="menuitemcheckbox"
              aria-checked={state === 'all' ? 'true' : state === 'some' ? 'mixed' : 'false'}
              onclick={() => toggleTag(p.ids, tag.name)}
            >
              <span class="tick" class:is-some={state === 'some'}>
                {#if state === 'all'}<Icon name="check" size={12} />{/if}
              </span>
              <span class="dot" style={`background:${tag.color}`}></span>
              <span class="pick-name">{tag.name}</span>
            </button>
          {/each}
        </div>
      {/if}
      <form class="pick-form" onsubmit={submitPicker}>
        <input class="input" placeholder="新標籤，Enter 加上" bind:value={pickerText} maxlength="40" data-autofocus />
      </form>
    {:else if p.kind === 'folder'}
      {@const current = commonFolder(p.ids)}
      <div class="pick-head xsmall muted">{p.ids.length > 1 ? `將 ${p.ids.length} 筆移到` : '移到'}</div>
      <div class="pick-list scroll" role="group" aria-label="資料夾">
        <button
          class="pick-item"
          onclick={() => {
            moveIds(p.ids, null);
            closePicker();
          }}
        >
          <span class="tick">{#if current === null}<Icon name="check" size={12} />{/if}</span>
          <span class="pick-name">未分類</span>
        </button>
        {#each records.folders as folder (folder.id)}
          <button
            class="pick-item"
            onclick={() => {
              moveIds(p.ids, folder);
              closePicker();
            }}
          >
            <span class="tick">{#if current === folder.id}<Icon name="check" size={12} />{/if}</span>
            <Icon name="folder" size={12} />
            <span class="pick-name">{folder.name}</span>
          </button>
        {/each}
      </div>
      <form class="pick-form" onsubmit={submitPicker}>
        <input class="input" placeholder="新資料夾，Enter 建立並移入" bind:value={pickerText} maxlength="40" data-autofocus />
      </form>
    {:else if p.kind === 'tag-color'}
      <div class="pick-head xsmall muted">「{p.tag.name}」的顏色</div>
      <div class="swatches">
        {#each TAG_COLORS as color (color)}
          <button
            class="swatch"
            class:is-on={records.tagColor(p.tag.name) === color}
            style={`background:${color}`}
            aria-label={color}
            aria-pressed={records.tagColor(p.tag.name) === color}
            onclick={() => {
              records.setTagColor(p.tag.name, color);
              closePicker();
            }}
          ></button>
        {/each}
      </div>
    {:else}
      <form class="pick-form stack-sm" onsubmit={submitPicker}>
        <label class="field-label" for="records-name-input">
          {p.kind === 'new-folder' ? '新增資料夾' : p.kind === 'rename-folder' ? '重新命名資料夾' : '重新命名標籤'}
        </label>
        <input
          id="records-name-input"
          class="input"
          class:is-invalid={pickerError !== ''}
          bind:value={pickerText}
          maxlength="40"
          data-autofocus
        />
        {#if pickerError}<span class="xsmall error-text">{pickerError}</span>{/if}
        {#if p.kind === 'rename-tag'}
          <span class="field-hint">改成已有的標籤名稱時，兩個標籤會合併。</span>
        {/if}
        <div class="row">
          <span class="spacer"></span>
          <button type="button" class="btn" onclick={closePicker}><Icon name="x" />取消</button>
          <button type="submit" class="btn btn--primary"><Icon name="check" />確定</button>
        </div>
      </form>
    {/if}
  </Popover>
{/if}

<style>
  /* 瀏覽器預覽提示只在沒有核心時出現，用 flex 讓清單永遠吃掉剩下的高度。 */
  .records {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .records-head {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--space-2);
    border-bottom: 1px solid var(--c-border);
  }

  .records-head .btn {
    min-height: 34px;
    padding: 0 var(--space-2);
    gap: var(--space-3);
    font-size: var(--fs-md);
  }

  /* 圖示固定佔同樣寬度，入口的文字對齊。 */
  .lead {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
  }

  .lead--disc {
    color: var(--c-text);
    background: var(--c-control-hover);
    border-radius: 999px;
  }

  /* 標題列與選取工具列互換：兩者高度相同（--swap-h），同時以相同曲線改變高度，
     總高度不變，清單不會跳動；內容另外淡入淡出並略為位移。 */
  .records {
    --swap-h: 38px;
    --swap-ease: cubic-bezier(0.3, 0.7, 0.2, 1);
  }

  .swap {
    flex: none;
    height: var(--swap-h);
    overflow: hidden;
    transition: height 260ms var(--swap-ease);
  }

  .swap.is-shut {
    height: 0;
  }

  .swap > * {
    transition:
      opacity 180ms ease 60ms,
      transform 260ms var(--swap-ease);
  }

  .swap.is-shut > * {
    opacity: 0;
    transition:
      opacity 120ms ease,
      transform 260ms var(--swap-ease);
  }

  .swap--title.is-shut > * {
    transform: translateY(-10px);
  }

  .swap--bar.is-shut > * {
    transform: translateY(8px);
  }

  @media (prefers-reduced-motion: reduce) {
    .swap,
    .swap > * {
      transition: none;
    }
  }

  .records-title {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    height: var(--swap-h);
    padding: 0 var(--space-3) var(--space-2);
    font-size: var(--fs-sm);
    font-weight: 600;
    letter-spacing: 0.04em;
    color: var(--c-text-dim);
  }

  .tools {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: 0 var(--space-2) var(--space-2);
  }

  /* 資料夾與標籤：可換行的小膠囊，太多時限制高度自己捲動。 */
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    max-height: 84px;
    overflow-y: auto;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    max-width: 100%;
    min-height: 24px;
    padding: 0 var(--space-2);
    font-size: var(--fs-xs);
    color: var(--c-text-dim);
    background: var(--c-panel);
    border: 1px solid var(--c-border);
    border-radius: 999px;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .chip:hover {
    color: var(--c-text);
    background: var(--c-control);
  }

  .chip.is-on {
    color: var(--c-text-strong);
    background: var(--c-control-hover);
    border-color: var(--c-border-strong);
  }

  .chip.is-drop {
    border-color: var(--c-focus);
    color: var(--c-text-strong);
  }

  .chip--icon {
    padding: 0 var(--space-1);
    min-width: 24px;
    justify-content: center;
  }

  .chip :global(svg) {
    flex: none;
  }

  .chip-count {
    font-family: var(--font-mono);
    color: var(--c-text-dim);
  }

  .dot {
    flex: none;
    width: 7px;
    height: 7px;
    border-radius: 999px;
  }

  .filter-row {
    display: flex;
    gap: var(--space-1);
  }

  .filter,
  .sort {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
    height: 28px;
    padding: 0 var(--space-2);
    color: var(--c-text-dim);
    background: var(--c-control);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-md);
  }

  .filter {
    flex: 1 1 auto;
  }

  .filter:focus-within,
  .sort:focus-within {
    border-color: var(--c-border-strong);
  }

  .filter-input,
  .sort-select {
    min-width: 0;
    height: 100%;
    padding: 0;
    font-size: var(--fs-sm);
    color: var(--c-text);
    background: none;
    border: none;
    outline: none;
  }

  .filter-input {
    flex: 1 1 auto;
  }

  /* 排序只顯示圖示＋目前選項，窄欄時縮短。 */
  .sort {
    flex: 0 1 112px;
  }

  .sort-select {
    width: 100%;
    cursor: pointer;
  }

  .sort-select option {
    background: var(--c-control);
  }

  .filter-clear {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 18px;
    height: 18px;
    padding: 0;
    color: var(--c-text-dim);
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .filter-clear:hover {
    color: var(--c-text);
    background: var(--c-control-hover);
  }

  /* 已經有自己的清除按鈕，隱藏瀏覽器內建的。 */
  .filter-input::-webkit-search-cancel-button {
    display: none;
  }

  .selection {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    height: calc(var(--swap-h) - var(--space-2));
    margin: 0 var(--space-2) var(--space-2);
    padding: 0 var(--space-1) 0 var(--space-2);
    background: var(--c-control);
    border: 1px solid var(--c-border-strong);
    border-radius: var(--radius-md);
  }

  .selection .btn {
    min-height: 26px;
  }

  .selection .btn--icon {
    min-width: 26px;
    padding: 0;
    background: none;
    border-color: transparent;
  }

  .selection .btn--icon:hover:not(:disabled) {
    background: var(--c-control-hover);
  }

  .records-note {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1);
    padding: 0 var(--space-3) var(--space-2);
  }

  .records-body {
    flex: 1 1 auto;
    min-height: 0;
  }

  .empty {
    padding: var(--space-2) var(--space-3);
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0 var(--space-2) var(--space-3);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .item {
    position: relative;
    display: flex;
    align-items: stretch;
    border-radius: var(--radius-md);
  }

  .item:hover,
  .item:focus-within {
    background: var(--c-control);
  }

  .item.is-active {
    background: var(--c-control-hover);
  }

  .item.is-selected {
    background: var(--c-control-hover);
  }

  .open {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1 1 auto;
    min-width: 0;
    padding: var(--space-2);
    padding-right: 36px;
    text-align: left;
    background: none;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    cursor: pointer;
  }

  .item.is-active .open {
    border-color: var(--c-border-strong);
  }

  .item.is-selected .open {
    border-color: var(--c-focus);
  }

  .item-title {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--c-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .item-title :global(svg) {
    flex: none;
  }

  .item.is-selected .item-title :global(svg) {
    color: var(--c-focus);
  }

  .item-title-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .item.is-active .item-title {
    color: var(--c-text-strong);
  }

  .item-preview {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding-left: 21px;
    color: var(--c-text-dim);
    overflow: hidden;
    white-space: nowrap;
  }

  .item-folder {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    min-width: 0;
    font-family: var(--font-sans);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .item-folder :global(svg) {
    flex: none;
  }

  .item-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 2px var(--space-2);
    padding-left: 21px;
    color: var(--c-text-dim);
  }

  .item-tag {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }

  /* 更多動作鈕平常隱藏；滑過、鍵盤聚焦或選單開著時一定看得到。 */
  .more {
    position: absolute;
    top: var(--space-1);
    right: var(--space-1);
    min-height: 28px;
    min-width: 28px;
    padding: 0;
    visibility: hidden;
    background: none;
    border-color: transparent;
    color: var(--c-text-dim);
  }

  .item:hover .more,
  .item:focus-within .more,
  .item.is-menu .more {
    visibility: visible;
  }

  .item.is-menu {
    background: var(--c-control);
  }

  .more:hover:not(:disabled),
  .item.is-menu .more {
    color: var(--c-text);
    background: var(--c-control-hover);
  }

  .records-foot {
    padding: var(--space-2);
    border-top: 1px solid var(--c-border);
  }

  .records-foot .btn {
    min-height: 34px;
    padding: 0 var(--space-2);
    gap: var(--space-3);
    font-size: var(--fs-md);
  }

  .confirm {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-2);
    border: 1px solid var(--c-danger);
    border-radius: var(--radius-md);
  }

  /* ---- 浮動面板內容 ---- */

  .pick-head {
    padding: var(--space-1) var(--space-2);
  }

  .pick-list {
    display: flex;
    flex-direction: column;
    min-height: 0;
    max-height: 240px;
  }

  .pick-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 30px;
    padding: 0 var(--space-2);
    font-size: var(--fs-sm);
    color: var(--c-text);
    text-align: left;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .pick-item:hover,
  .pick-item:focus-visible {
    background: var(--c-control-hover);
  }

  .pick-item:focus-visible {
    outline-offset: -2px;
  }

  .pick-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tick {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 14px;
    height: 14px;
    color: var(--c-text-strong);
    border: 1px solid var(--c-border-strong);
    border-radius: var(--radius-sm);
  }

  /* 部分選取的紀錄有這個標籤：中間一條短線。 */
  .tick.is-some::after {
    content: '';
    width: 8px;
    height: 2px;
    background: var(--c-text);
  }

  .pick-form {
    padding: var(--space-1);
    border-top: 1px solid var(--c-border);
    margin-top: var(--space-1);
  }

  .pick-form:first-child {
    border-top: none;
    margin-top: 0;
  }

  .pick-form .input {
    padding: var(--space-1) var(--space-2);
  }

  .error-text {
    color: var(--c-danger);
  }

  .swatches {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: var(--space-2);
    padding: var(--space-2);
  }

  .swatch {
    aspect-ratio: 1;
    border: 2px solid var(--c-control);
    border-radius: 999px;
    cursor: pointer;
  }

  .swatch.is-on {
    box-shadow: 0 0 0 2px var(--c-text);
  }
</style>
