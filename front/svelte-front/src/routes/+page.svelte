{#if uploading}
  <div class="fixed bottom-24 right-6 w-80 bg-white shadow-2xl rounded-xl border border-indigo-100 p-4 z-50 animate-in fade-in slide-in-from-bottom-4">
    <div class="flex items-center justify-between mb-2">
      <p class="text-sm font-bold text-indigo-700">
        {currentFileIndex + 1} / {totalFilesCount} 번째 업로드 중
      </p>
      <span class="text-xs font-mono font-bold text-indigo-500">{uploadProgress}%</span>
    </div>
    
    <p class="text-xs text-gray-500 truncate mb-3">{currentUploadingFileName}</p>
    
    <div class="w-full bg-gray-100 h-2 rounded-full overflow-hidden">
      <div 
        class="bg-indigo-600 h-full transition-all duration-300 ease-out" 
        style="width: {uploadProgress}%"
      ></div>
    </div>
  </div>
{/if}

<script lang="ts">
  import StorageWidget from './StorageWidget.svelte'; 
  import { storageState } from '$lib/states/storage.svelte.js';
  import axios from 'axios';
  import { format } from 'date-fns';
  import { getNasUrl } from '$lib/constants';
  import { 
    Folder, File as FileIcon, Upload, Home, ArrowLeft, Trash2, 
    FileVideo, FileAudio, FileImage, FileText, FileCode, 
    FileArchive, FileSearch, Download, HardDrive, RefreshCw, 
    FolderPlus, FileSpreadsheet, FilePieChart, FileType2, FileType, FileJson,
    Captions, FileChartColumn, Users, Globe, Lock, UserPlus,
    Pencil, FolderInput, RotateCcw, XCircle, Search, Eye, X,
    ChevronLeft, ChevronRight, Maximize, Minimize
  } from 'lucide-svelte';

  const API_BASE = getNasUrl();

  // 인증은 HttpOnly 세션 쿠키로 이루어진다. 모든 요청에 쿠키를 동봉한다.
  axios.defaults.withCredentials = true;

  function apiHeaders(extra: Record<string, string> = {}) {
    return { ...extra };
  }

  interface FileItem {
    id: string;
    name: string;
    is_dir: boolean;
    size: number;
    created_at: string;
    path?: string | null;
    preview_url?: string | null;
  }

  interface CrewEntry {
    id: string;
    name: string;
    visibility: 'Public' | 'Private';
    root_folder_id: string | null;
    is_member: boolean;
  }

  // --- Svelte 5 Runes (상태 관리) ---
  let currentFolderId = $state<string | null>(null);
  let folderHistory = $state<{id: string | null, name: string}[]>([]);
  let items = $state<FileItem[]>([]);
  let crews = $state<CrewEntry[]>([]);
  let canWrite = $state(true);
  let loading = $state(false);
  let uploading = $state(false);
  let error = $state<string | null>(null);
  let uploadProgress = $state(0);
  let currentFileIndex = $state(0);
  let totalFilesCount = $state(0);
  let currentUploadingFileName = $state('');
  
  // 💡 다중 선택을 위한 Set 상태
  let selectedIds = $state<Set<string>>(new Set());
  let trashMode = $state(false);
  let searchQuery = $state('');
  let searchActive = $state(false);
  let searchResults = $state<FileItem[]>([]);
  let previewItem = $state<FileItem | null>(null);
  let dragOver = $state(false);
  let previewFullscreen = $state(false);
  let subtitleTracks = $state<{ label: string; src: string }[]>([]);
  let previewShell = $state<HTMLDivElement | null>(null);
  let previewVideoEl = $state<HTMLVideoElement | null>(null);
  let previewImageUrl = $state<string | null>(null);
  let previewImageLoading = $state(false);
  let previewImageError = $state<string | null>(null);

  let displayItems = $derived(searchActive ? searchResults : items);
  let selectableForAll = $derived(
    trashMode ? displayItems : displayItems.filter((i) => !i.is_dir)
  );
  let imagePreviewItems = $derived(
    displayItems.filter((i) => !i.is_dir && isImagePreview(i))
  );
  let previewImageIndex = $derived(
    previewItem && isImagePreview(previewItem)
      ? imagePreviewItems.findIndex((i) => i.id === previewItem!.id)
      : -1
  );
  let canPreviewPrev = $derived(previewImageIndex > 0);
  let canPreviewNext = $derived(
    previewImageIndex >= 0 && previewImageIndex < imagePreviewItems.length - 1
  );

  const PREVIEW_IMAGE_EXT = ['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg'];
  const PREVIEW_VIDEO_EXT = ['mp4', 'mkv', 'webm', 'mov', 'm4v', 'avi'];
  const VIDEO_SEEK_SECONDS = 10;

  function currentDisplayItems(): FileItem[] {
    return searchActive ? searchResults : items;
  }

  function streamUrlFor(item: FileItem): string {
    if (item.preview_url) {
      return item.preview_url.startsWith('http')
        ? item.preview_url
        : `${API_BASE}${item.preview_url}`;
    }
    return `${API_BASE}/api/files/${item.id}/stream?inline=true`;
  }

  function isPreviewable(item: FileItem): boolean {
    if (item.is_dir) return false;
    if (item.preview_url) return true;
    const ext = item.name.split('.').pop()?.toLowerCase() || '';
    return PREVIEW_IMAGE_EXT.includes(ext) || PREVIEW_VIDEO_EXT.includes(ext);
  }

  function isVideoPreview(item: FileItem): boolean {
    const ext = item.name.split('.').pop()?.toLowerCase() || '';
    return PREVIEW_VIDEO_EXT.includes(ext);
  }

  function isImagePreview(item: FileItem): boolean {
    const ext = item.name.split('.').pop()?.toLowerCase() || '';
    return PREVIEW_IMAGE_EXT.includes(ext);
  }

  function vttUrlFor(path: string): string {
    return path.startsWith('http') ? path : `${API_BASE}${path}`;
  }

  function revokePreviewImageUrl() {
    if (previewImageUrl) {
      URL.revokeObjectURL(previewImageUrl);
      previewImageUrl = null;
    }
  }

  async function loadPreviewImage(item: FileItem) {
    revokePreviewImageUrl();
    previewImageError = null;
    if (!isImagePreview(item)) return;

    previewImageLoading = true;
    try {
      const res = await axios.get(`${API_BASE}/api/files/${item.id}`, {
        responseType: 'blob',
        headers: apiHeaders(),
      });
      previewImageUrl = URL.createObjectURL(res.data);
    } catch (err) {
      const status = axios.isAxiosError(err) ? err.response?.status : undefined;
      if (status === 401 || status === 403) {
        previewImageError = '이미지를 볼 권한이 없습니다. 로그인 상태를 확인하세요.';
      } else if (status === 404) {
        previewImageError = '파일을 찾을 수 없습니다.';
      } else {
        previewImageError = '이미지를 불러오지 못했습니다.';
      }
    } finally {
      previewImageLoading = false;
    }
  }

  async function loadSubtitleTracks(videoId: string) {
    subtitleTracks = [];
    try {
      const res = await axios.get<{ id: string; name: string; label: string; vtt_url: string }[]>(
        `${API_BASE}/api/files/${videoId}/subtitles`,
        { headers: apiHeaders() }
      );
      subtitleTracks = res.data.map((t) => ({
        label: t.label || t.name,
        src: vttUrlFor(t.vtt_url),
      }));
    } catch {
      subtitleTracks = [];
    }
  }

  function openPreview(item: FileItem) {
    previewItem = item;
    if (isVideoPreview(item)) {
      revokePreviewImageUrl();
      previewImageError = null;
      loadSubtitleTracks(item.id);
    } else if (isImagePreview(item)) {
      subtitleTracks = [];
      loadPreviewImage(item);
    } else {
      subtitleTracks = [];
      revokePreviewImageUrl();
      previewImageError = null;
    }
  }

  function navigatePreview(delta: number) {
    const idx = previewImageIndex;
    if (idx < 0) return;
    const next = imagePreviewItems[idx + delta];
    if (next) openPreview(next);
  }

  async function togglePreviewFullscreen() {
    if (!previewShell) return;
    try {
      if (!document.fullscreenElement) {
        await previewShell.requestFullscreen();
        previewFullscreen = true;
      } else {
        await document.exitFullscreen();
        previewFullscreen = false;
      }
    } catch {
      previewFullscreen = !previewFullscreen;
    }
  }

  function openItem(item: FileItem) {
    if (trashMode) return;
    if (item.is_dir) {
      enterFolder(item.id, item.name);
    } else if (isPreviewable(item)) {
      openPreview(item);
    } else {
      handleDownload(item.id);
    }
  }

  function isTypingTarget(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false;
    const tag = target.tagName;
    return tag === 'INPUT' || tag === 'TEXTAREA' || target.isContentEditable;
  }

  function seekPreviewVideo(deltaSeconds: number) {
    const video = previewVideoEl;
    if (!video || !Number.isFinite(video.duration)) return;
    video.currentTime = Math.min(
      Math.max(0, video.currentTime + deltaSeconds),
      video.duration
    );
  }

  function togglePreviewVideoPlay() {
    const video = previewVideoEl;
    if (!video) return;
    if (video.paused) {
      void video.play();
    } else {
      video.pause();
    }
  }

  function closePreview() {
    previewVideoEl?.pause();
    previewItem = null;
    subtitleTracks = [];
    revokePreviewImageUrl();
    previewImageError = null;
    previewFullscreen = false;
    if (document.fullscreenElement) {
      document.exitFullscreen().catch(() => {});
    }
  }

  $effect(() => {
    if (!previewItem) return;
    const item = previewItem;

    const onKey = (e: KeyboardEvent) => {
      if (isTypingTarget(e.target)) return;

      if (e.key === 'Escape') {
        closePreview();
        return;
      }

      if (isVideoPreview(item) && previewVideoEl) {
        if (e.key === 'ArrowLeft') {
          e.preventDefault();
          e.stopImmediatePropagation();
          seekPreviewVideo(-VIDEO_SEEK_SECONDS);
          return;
        }
        if (e.key === 'ArrowRight') {
          e.preventDefault();
          e.stopImmediatePropagation();
          seekPreviewVideo(VIDEO_SEEK_SECONDS);
          return;
        }
        if (e.key === ' ' || e.code === 'Space') {
          e.preventDefault();
          e.stopImmediatePropagation();
          togglePreviewVideoPlay();
          return;
        }
      }

      if (isImagePreview(item) && previewImageIndex >= 0) {
        if (e.key === 'ArrowLeft' && canPreviewPrev) {
          e.preventDefault();
          navigatePreview(-1);
        } else if (e.key === 'ArrowRight' && canPreviewNext) {
          e.preventDefault();
          navigatePreview(1);
        } else if (e.key === 'f' || e.key === 'F') {
          e.preventDefault();
          togglePreviewFullscreen();
        }
      }
    };
    // capture: 브라우저 <video controls> 기본 단축키(전체화면 시 특히)보다 먼저 처리
    document.addEventListener('keydown', onKey, true);
    return () => document.removeEventListener('keydown', onKey, true);
  });

  $effect(() => {
    const onFullscreenChange = () => {
      previewFullscreen = document.fullscreenElement === previewShell;
    };
    document.addEventListener('fullscreenchange', onFullscreenChange);
    return () => document.removeEventListener('fullscreenchange', onFullscreenChange);
  });

  // --- 비즈니스 로직 ---
  function getFileInfo(fileName: string) {
    const ext = fileName.split('.').pop()?.toLowerCase() || '';
    
    if (['xlsx', 'xls', 'csv', 'cell'].includes(ext)) {
      return { icon: FileSpreadsheet, color: 'bg-green-100 text-green-700' };
    }
    if (['docx', 'doc', 'hwpw'].includes(ext)) {
      return { icon: FileType2, color: 'bg-blue-100 text-blue-700' };
    }
    if (['pptx', 'ppt', 'show'].includes(ext)) {
      return { icon: FileChartColumn, color: 'bg-orange-100 text-orange-700' };
    }
    if (['hwp', 'hwpx'].includes(ext)) {
      return { icon: FileType, color: 'bg-sky-100 text-sky-700' };
    }
    if (ext === 'pdf') {
      return { icon: FileText, color: 'bg-red-100 text-red-600' };
    }
    if (['mp4', 'mkv', 'avi', 'mov', 'webm'].includes(ext)) {
      return { icon: FileVideo, color: 'bg-purple-100 text-purple-600' };
    }
    if (['srt', 'ass', 'vtt', 'smi', 'ssa'].includes(ext)) {
      return { icon: Captions, color: 'bg-pink-100 text-pink-600' };
    }
    if (['mp3', 'wav', 'flac', 'ogg', 'm4a'].includes(ext)) {
      return { icon: FileAudio, color: 'bg-amber-100 text-amber-600' };
    }
    if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg'].includes(ext)) {
      return { icon: FileImage, color: 'bg-emerald-100 text-emerald-600' };
    }
    if (['rs', 'js', 'ts', 'html', 'css', 'py', 'sh', 'cpp'].includes(ext)) {
      return { icon: FileCode, color: 'bg-slate-100 text-slate-700' };
    }
    if (['json', 'yaml', 'toml', 'xml'].includes(ext)) {
      return { icon: FileJson, color: 'bg-yellow-100 text-yellow-700' };
    }
    if (['zip', 'rar', '7z', 'tar', 'gz'].includes(ext)) {
      return { icon: FileArchive, color: 'bg-zinc-100 text-zinc-600' };
    }
    return { icon: FileIcon, color: 'bg-gray-100 text-gray-500' };
  }

  // 💡 개별 선택/해제 (Svelte 5 반응성 적용)
  function toggleSelection(id: string) {
    const nextIds = new Set(selectedIds);
    if (nextIds.has(id)) {
      nextIds.delete(id);
    } else {
      nextIds.add(id);
    }
    selectedIds = nextIds; // 새로운 Set 할당
  }

  // 💡 전체 선택/해제 (Svelte 5 반응성 적용)
  function toggleAll() {
    const displayItems = currentDisplayItems();
    const selectableItems = trashMode
      ? displayItems
      : displayItems.filter((item) => !item.is_dir);
    const nextIds = new Set<string>();

    if (selectedIds.size !== selectableItems.length && selectableItems.length > 0) {
      selectableItems.forEach((item) => nextIds.add(item.id));
    }
    selectedIds = nextIds;
  }

  async function handleBulkRestore() {
    const displayItems = currentDisplayItems();
    const targets = displayItems.filter((item) => selectedIds.has(item.id));
    if (targets.length === 0) return;
    if (!confirm(`선택한 ${targets.length}개 항목을 복구하시겠습니까?`)) return;
    loading = true;
    try {
      for (const item of targets) {
        await axios.post(
          `${API_BASE}/api/trash/restore`,
          { id: item.id, is_dir: item.is_dir, folder_id: currentFolderId },
          { headers: apiHeaders() }
        );
      }
      selectedIds = new Set();
      await fetchFiles(currentFolderId);
    } catch {
      alert('일부 항목 복구에 실패했습니다.');
    } finally {
      loading = false;
    }
  }

  async function handleBulkPermanentDelete() {
    const displayItems = currentDisplayItems();
    const targets = displayItems.filter((item) => selectedIds.has(item.id));
    if (targets.length === 0) return;
    if (!confirm(`선택한 ${targets.length}개 항목을 영구 삭제하시겠습니까?\n이 작업은 되돌릴 수 없습니다.`)) return;
    loading = true;
    try {
      for (const item of targets) {
        await axios.post(
          `${API_BASE}/api/trash/permanent-delete`,
          { id: item.id, is_dir: item.is_dir, folder_id: currentFolderId },
          { headers: apiHeaders() }
        );
      }
      selectedIds = new Set();
      await fetchFiles(currentFolderId);
    } catch {
      alert('일부 항목 영구 삭제에 실패했습니다.');
    } finally {
      loading = false;
    }
  }

  async function handleMultiDownload() {
    if (selectedIds.size === 0) return;
    try {
      loading = true; 
      const response = await axios.post(
        `${API_BASE}/api/files/download-zip`, 
        Array.from(selectedIds), 
        { 
            responseType: 'blob',
            headers: apiHeaders({ 'Content-Type': 'application/json' })
        }
      );

      const blob = new Blob([response.data], { type: 'application/zip' });
      const url = window.URL.createObjectURL(blob);

      const link = document.createElement('a');
      link.href = url;
      const timestamp = format(new Date(), 'yyyyMMdd_HHmm');
      link.download = `NAS_Export_${timestamp}.zip`;
      
      document.body.appendChild(link);
      link.click();

      link.remove();
      window.URL.revokeObjectURL(url);
      
      // 💡 다운로드 완료 후 선택 해제
      selectedIds = new Set(); 
    } catch (err) {
      console.error("다중 다운로드 실패:", err);
      alert("파일을 압축하는 중 오류가 발생했습니다. 서버 용량이나 파일 상태를 확인하세요.");
    } finally {
      loading = false;
    }
  }

  // 파일 목록 가져오기
  async function fetchFiles(folderId: string | null) {
    selectedIds = new Set();
    loading = true;
    error = null;
    searchActive = false;
    try {
      if (trashMode) {
        const url = folderId
          ? `${API_BASE}/api/trash?folder_id=${encodeURIComponent(folderId)}`
          : `${API_BASE}/api/trash`;
        const response = await axios.get<FileItem[]>(url, { headers: apiHeaders() });
        items = response.data.sort((a, b) => {
          if (a.is_dir === b.is_dir) return a.name.localeCompare(b.name);
          return a.is_dir ? -1 : 1;
        });
        return;
      }

      const url = folderId 
        ? `${API_BASE}/api/files?folder_id=${folderId}` 
        : `${API_BASE}/api/files`;
      
      const response = await axios.get<FileItem[]>(url, { headers: apiHeaders() });
      items = response.data.sort((a, b) => {
        if (a.is_dir === b.is_dir) return a.name.localeCompare(b.name);
        return a.is_dir ? -1 : 1;
      });
    } catch (err) {
      error = trashMode
        ? '휴지통을 불러오지 못했습니다. 권한을 확인하세요.'
        : '서버 연결 실패. 백엔드가 실행 중인지 확인하세요.';
    } finally {
      loading = false;
    }
  }

  async function runSearch() {
    const q = searchQuery.trim();
    if (q.length < 2) {
      alert('검색어는 2자 이상 입력하세요.');
      return;
    }
    loading = true;
    error = null;
    try {
      const res = await axios.get<FileItem[]>(
        `${API_BASE}/api/files/search?q=${encodeURIComponent(q)}`,
        { headers: apiHeaders() }
      );
      searchResults = res.data;
      searchActive = true;
      trashMode = false;
    } catch {
      alert('검색에 실패했습니다.');
    } finally {
      loading = false;
    }
  }

  function clearSearch() {
    searchActive = false;
    searchQuery = '';
    searchResults = [];
    fetchFiles(currentFolderId);
  }

  function toggleTrashMode() {
    trashMode = !trashMode;
    searchActive = false;
    fetchFiles(currentFolderId);
  }

  async function handleRename(item: FileItem) {
    const newName = prompt('새 이름을 입력하세요:', item.name);
    if (!newName || newName.trim() === item.name) return;
    try {
      const endpoint = item.is_dir ? 'folders' : 'files';
      await axios.patch(
        `${API_BASE}/api/${endpoint}/${item.id}`,
        { name: newName.trim() },
        { headers: apiHeaders() }
      );
      await fetchFiles(currentFolderId);
    } catch {
      alert('이름 변경에 실패했습니다.');
    }
  }

  async function handleMove(item: FileItem) {
    const destLabel = currentFolderId ? '현재 폴더' : '홈(루트)';
    if (!confirm(`'${item.name}'을(를) ${destLabel}로 이동하시겠습니까?`)) return;
    try {
      if (item.is_dir) {
        await axios.patch(
          `${API_BASE}/api/folders/${item.id}`,
          { parent_id: currentFolderId, update_parent_id: true },
          { headers: apiHeaders() }
        );
      } else {
        await axios.patch(
          `${API_BASE}/api/files/${item.id}`,
          { folder_id: currentFolderId, update_folder_id: true },
          { headers: apiHeaders() }
        );
      }
      await fetchFiles(currentFolderId);
    } catch {
      alert('이동에 실패했습니다. 같은 이름이 있거나 권한이 없을 수 있습니다.');
    }
  }

  async function handleRestore(item: FileItem) {
    if (!confirm(`'${item.name}'을(를) 복구하시겠습니까?`)) return;
    try {
      await axios.post(
        `${API_BASE}/api/trash/restore`,
        { id: item.id, is_dir: item.is_dir, folder_id: currentFolderId },
        { headers: apiHeaders() }
      );
      await fetchFiles(currentFolderId);
    } catch {
      alert('복구에 실패했습니다. 상위 폴더가 삭제된 경우 상위 폴더를 먼저 복구하세요.');
    }
  }

  async function handlePermanentDelete(item: FileItem) {
    if (!confirm(`'${item.name}'을(를) 영구 삭제하시겠습니까?\n이 작업은 되돌릴 수 없습니다.`)) return;
    try {
      await axios.post(
        `${API_BASE}/api/trash/permanent-delete`,
        { id: item.id, is_dir: item.is_dir, folder_id: currentFolderId },
        { headers: apiHeaders() }
      );
      await fetchFiles(currentFolderId);
    } catch {
      alert('영구 삭제에 실패했습니다.');
    }
  }

  // 홈(루트)에서만 함께 보여줄 Crew 목록을 가져온다.
  // 공개 Crew는 누구에게나, 비공개 Crew는 멤버에게만 내려온다(백엔드에서 필터링).
  async function fetchCrews() {
    try {
      const res = await axios.get<CrewEntry[]>(`${API_BASE}/api/crews/visible`, { headers: apiHeaders() });
      crews = res.data;
    } catch (err) {
      crews = [];
    }
  }

  // 현재 위치에서 쓰기 권한이 있는지 조회해, 업로드/새 폴더/휴지통 버튼 노출을 결정한다.
  async function fetchAccess(folderId: string | null) {
    try {
      const url = folderId
        ? `${API_BASE}/api/folders/access?folder_id=${encodeURIComponent(folderId)}`
        : `${API_BASE}/api/folders/access`;
      const res = await axios.get<{ can_write: boolean }>(url, { headers: apiHeaders() });
      canWrite = !!res.data.can_write;
    } catch (err) {
      canWrite = false;
    }
  }

  async function refreshAll() {
    try {
      await Promise.all([
        fetchFiles(currentFolderId),
        fetchAccess(currentFolderId),
        currentFolderId === null ? fetchCrews() : Promise.resolve(),
        storageState.refresh() 
      ]);
    } catch (err) {
      console.error("데이터 갱신 중 오류 발생:", err);
    }
  }

  $effect(() => {
    fetchFiles(currentFolderId);
    fetchAccess(currentFolderId);
    if (currentFolderId === null) {
      fetchCrews();
    } else {
      crews = [];
    }
  });

  function enterCrew(crew: CrewEntry) {
    if (!crew.root_folder_id) return;
    enterFolder(crew.root_folder_id, crew.name);
  }

  async function handleJoinCrew(crew: CrewEntry) {
    if (!confirm(`'${crew.name}' 크루에 가입 신청하시겠습니까?`)) return;
    try {
      await axios.post(`${API_BASE}/api/crews/${crew.id}/join`, null, { headers: apiHeaders() });
      alert('가입 신청이 접수되었습니다. 관리자 승인 후 이용할 수 있습니다.');
    } catch (err) {
      const status = axios.isAxiosError(err) ? err.response?.status : undefined;
      if (status === 401) {
        alert('로그인이 필요합니다.');
      } else if (status === 400) {
        alert('이미 신청했거나 멤버입니다.');
      } else {
        alert('가입 신청에 실패했습니다.');
      }
    }
  }

  async function handleCreateFolder() {
    const folderName = prompt('새 폴더 이름을 입력하세요:', '새 폴더');
    if (!folderName) return;

    try {
      await axios.post(`${API_BASE}/api/folders`, {
        name: folderName.trim(),
        parent_id: currentFolderId
      }, { headers: apiHeaders() });
      fetchFiles(currentFolderId);
    } catch {
      alert('폴더 생성 실패');
    }
  }

  async function handleUploadFiles(files: globalThis.File[]) {
    if (!files.length) return;

    uploading = true;
    totalFilesCount = files.length;
    let successCount = 0;

    for (let i = 0; i < files.length; i++) {
      currentFileIndex = i;
      currentUploadingFileName = files[i].name;
      try {
        const url = `${API_BASE}/api/upload/${encodeURIComponent(files[i].name)}${currentFolderId ? `?folder_id=${currentFolderId}` : ''}`;

        await axios.post(url, files[i], {
          headers: apiHeaders({
            'Content-Type': 'application/octet-stream',
            'X-File-Size': files[i].size.toString()
          }),
          onUploadProgress: (p) => {
            const total = p.total || files[i].size;
            uploadProgress = Math.round((p.loaded * 100) / total);
          }
        });
        successCount++;
      } catch (err) {
        console.error(`${files[i].name} 업로드 실패:`, err);
      }
    }
    uploading = false;
    uploadProgress = 0;
    currentFileIndex = 0;
    await refreshAll();

    if (successCount > 0) {
      alert(`${successCount}개의 파일이 성공적으로 업로드 되었습니다!`);
    } else {
      alert('업로드에 실패했습니다.');
    }
  }

  async function handleUpload(event: Event) {
    const input = event.target as HTMLInputElement;
    if (!input.files?.length) return;
    await handleUploadFiles(Array.from(input.files));
    input.value = '';
  }

  function handleDragOver(event: DragEvent) {
    if (!canWrite || trashMode || searchActive) return;
    event.preventDefault();
    dragOver = true;
  }

  function handleDragLeave() {
    dragOver = false;
  }

  async function handleDrop(event: DragEvent) {
    event.preventDefault();
    dragOver = false;
    if (!canWrite || trashMode || searchActive) return;
    const files = event.dataTransfer?.files;
    if (!files?.length) return;
    await handleUploadFiles(Array.from(files));
  }

  async function handleDelete(item: FileItem) {
    const typeName = item.is_dir ? '폴더' : '파일';
    if (!confirm(`정말로 이 ${typeName}('${item.name}')을(를) 삭제하시겠습니까?`)) return;

    try {
      const endpoint = item.is_dir ? 'folders' : 'files';
      await axios.delete(`${API_BASE}/api/${endpoint}/${item.id}`, { headers: apiHeaders() });
      fetchFiles(currentFolderId);
    } catch {
      alert(`${typeName} 삭제 실패`);
    }
  }

  async function handleEmptyTrash() {
    if (!confirm('휴지통을 비우시겠습니까?\n이 작업은 되돌릴 수 없으며 모든 데이터가 영구 삭제됩니다.')) return;

    loading = true;
    try {
      // 현재 위치(Crew)의 휴지통만 비운다. 루트면 개인/전역 스코프.
      const url = currentFolderId
        ? `${API_BASE}/api/nas/empty-trash?folder_id=${encodeURIComponent(currentFolderId)}`
        : `${API_BASE}/api/nas/empty-trash`;
      const response = await axios.post(url, null, { headers: apiHeaders() });
      if (response.data.success) {
        alert(response.data.message);
        refreshAll();
      }
    } catch (e) {
      const status = axios.isAxiosError(e) ? e.response?.status : undefined;
      const serverMsg = axios.isAxiosError(e)
        ? (e.response?.data as { message?: string } | undefined)?.message
        : undefined;
      if (status === 401) {
        alert('세션이 만료되었거나 로그인되어 있지 않습니다. 다시 로그인한 뒤 시도해주세요.');
      } else if (status === 403) {
        alert('이 위치의 휴지통을 비울 권한이 없습니다. (소유자/매니저만 가능합니다)');
      } else {
        alert(serverMsg ? `휴지통 비우기 실패: ${serverMsg}` : '휴지통 비우기 실패');
      }
    } finally {
      loading = false;
    }
  }

  function handleDownload(id: string) {
    window.open(`${API_BASE}/api/files/${id}`, '_blank');
  }

  const enterFolder = (id: string, name: string) => {
    if (searchActive) clearSearch();
    folderHistory = [...folderHistory, { id: currentFolderId, name }];
    currentFolderId = id;
  };

  const goUp = () => {
    if (searchActive) clearSearch();
    const previous = folderHistory.pop();
    if (previous !== undefined) currentFolderId = previous.id;
  };

  const goToHome = () => {
    if (searchActive) clearSearch();
    currentFolderId = null;
    folderHistory = [];
  };

  function formatSize(bytes: number) {
    if (bytes === 0) return '-';
    const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
    return (bytes / Math.pow(1024, i)).toFixed(2) + ' ' + sizes[i];
  }
</script>

<div class="min-h-screen bg-gray-50 text-gray-800 font-sans selection:bg-indigo-100">
  <header class="bg-white border-b border-gray-200 px-6 py-4 flex flex-col md:flex-row md:items-center justify-between sticky top-0 z-10 shadow-sm gap-4">
    <div class="flex items-center gap-4 flex-1 overflow-hidden">
      <div class="flex items-center gap-2 text-indigo-600 shrink-0">
        <HardDrive size={28} />
        <h1 class="text-xl font-bold tracking-tight">Linux Rust NAS</h1>
      </div>
      
      <div class="flex items-center bg-gray-100 px-3 py-1.5 rounded-lg text-sm text-gray-600 overflow-x-auto whitespace-nowrap">
        <button onclick={goToHome} class="flex items-center hover:text-indigo-600 transition-colors {!currentFolderId ? 'font-bold text-indigo-600' : ''}">
          <Home size={14} class="mr-1" /> Home
        </button>
        {#each folderHistory as folder}
          <span class="mx-2 text-gray-400">/</span>
          <span class="font-medium text-gray-700">{folder.name}</span>
        {/each}
      </div>
    </div>

    <div class="flex items-center gap-3">
      <div class="flex items-center gap-2 bg-gray-100 rounded-lg px-2 py-1.5">
        <Search size={16} class="text-gray-400 shrink-0" />
        <input
          type="text"
          bind:value={searchQuery}
          placeholder="파일 검색..."
          class="bg-transparent text-sm outline-none w-32 sm:w-48"
          onkeydown={(e) => e.key === 'Enter' && runSearch()}
        />
        <button
          onclick={runSearch}
          class="text-xs font-semibold text-indigo-600 hover:text-indigo-800 px-2"
        >
          검색
        </button>
        {#if searchActive}
          <button onclick={clearSearch} class="text-xs text-gray-500 hover:text-gray-700 px-1">취소</button>
        {/if}
      </div>

      <button 
        onclick={() => fetchFiles(currentFolderId)} 
        class="p-2 text-gray-500 hover:bg-gray-100 rounded-full transition-colors"
        title="새로고침"
      >
        <RefreshCw size={20} class={loading ? "animate-spin text-indigo-600" : ""} />
      </button>

      {#if canWrite}
        <button
          onclick={toggleTrashMode}
          class="flex items-center gap-2 px-3 py-2 rounded-lg transition-colors shadow-sm text-sm font-medium border {trashMode ? 'bg-red-50 border-red-300 text-red-700' : 'bg-white border-gray-200 text-gray-700 hover:bg-gray-50'}"
        >
          <Trash2 size={18} class={trashMode ? 'text-red-600' : 'text-gray-500'} />
          <span class="hidden sm:inline">{trashMode ? '휴지통 보기 중' : '휴지통'}</span>
        </button>

        {#if trashMode}
        <button 
          onclick={handleEmptyTrash} 
          disabled={loading}
          class="flex items-center gap-2 px-3 py-2 bg-white border border-red-200 text-red-600 rounded-lg hover:bg-red-50 transition-colors shadow-sm text-sm font-medium disabled:opacity-50"
        >
          <Trash2 size={18} class="text-red-500" />
          <span class="hidden sm:inline">전체 비우기</span>
        </button>
        {/if}

        {#if !trashMode}
        <button 
          onclick={handleCreateFolder} 
          class="flex items-center gap-2 px-3 py-2 bg-white border border-gray-200 text-gray-700 rounded-lg hover:bg-gray-50 transition-colors shadow-sm text-sm font-medium"
        >
          <FolderPlus size={18} class="text-indigo-500" />
          <span>새 폴더</span>
        </button>
        {/if}
      {/if}

      {#if selectedIds.size > 0 && !trashMode}
        <button 
          onclick={handleMultiDownload}
          disabled={loading}
          class="flex items-center gap-2 px-4 py-2 bg-indigo-600 text-white rounded-lg hover:bg-indigo-700 transition-all shadow-md animate-in zoom-in group"
        >
          {#if loading}
            <RefreshCw size={18} class="animate-spin" />
            <span class="text-sm font-bold">압축 중...</span>
          {:else}
            <Download size={18} class="group-hover:scale-110 transition-transform" />
            <span class="text-sm font-bold">{selectedIds.size}개 다운로드</span>
          {/if}
        </button>
      {/if}

      {#if trashMode && selectedIds.size > 0 && canWrite}
        <button
          onclick={handleBulkRestore}
          disabled={loading}
          class="flex items-center gap-2 px-3 py-2 bg-emerald-600 text-white rounded-lg hover:bg-emerald-700 text-sm font-medium disabled:opacity-50"
        >
          <RotateCcw size={16} />
          <span>{selectedIds.size}개 복구</span>
        </button>
        <button
          onclick={handleBulkPermanentDelete}
          disabled={loading}
          class="flex items-center gap-2 px-3 py-2 bg-red-600 text-white rounded-lg hover:bg-red-700 text-sm font-medium disabled:opacity-50"
        >
          <XCircle size={16} />
          <span>{selectedIds.size}개 영구 삭제</span>
        </button>
      {/if}

      {#if canWrite && !trashMode}
        <label class="relative flex items-center gap-2 px-4 py-2 bg-gray-100 text-indigo-700 rounded-lg cursor-pointer hover:bg-gray-200 transition-colors shadow-sm overflow-hidden min-w-[100px] justify-center">
          <Upload size={18} />
          <span class="font-medium text-sm">업로드</span>
          <input type="file" class="hidden" onchange={handleUpload} disabled={uploading} multiple />
        </label>
      {/if}
    </div>
  </header>

  <main
    class="p-4 md:p-6 max-w-7xl mx-auto relative"
    class:ring-2={dragOver}
    class:ring-indigo-400={dragOver}
    class:ring-inset={dragOver}
    ondragover={handleDragOver}
    ondragleave={handleDragLeave}
    ondrop={handleDrop}
  >
    {#if dragOver && canWrite && !trashMode && !searchActive}
      <div class="absolute inset-0 z-20 bg-indigo-50/80 border-2 border-dashed border-indigo-400 rounded-xl flex items-center justify-center pointer-events-none">
        <p class="text-indigo-700 font-semibold text-lg">여기에 파일을 놓아 업로드</p>
      </div>
    {/if}
    {#if currentFolderId}
      <div class="mb-4">
        <button onclick={goUp} class="flex items-center gap-2 text-gray-600 hover:text-indigo-700 bg-white border border-gray-200 px-3 py-1.5 rounded-lg shadow-sm text-sm font-medium">
          <ArrowLeft size={16} />
          <span>상위 폴더</span>
        </button>
      </div>
    {/if}

    {#if searchActive}
      <div class="mb-4 flex items-center justify-between bg-indigo-50 border border-indigo-100 rounded-lg px-4 py-2 text-sm text-indigo-800">
        <span>검색 결과: <strong>{searchResults.length}</strong>개</span>
        <button onclick={clearSearch} class="text-indigo-600 hover:text-indigo-800 font-medium">목록으로 돌아가기</button>
      </div>
    {:else if trashMode}
      <div class="mb-4 bg-red-50 border border-red-100 rounded-lg px-4 py-2 text-sm text-red-700">
        휴지통 보기 중입니다. 항목을 복구하거나 영구 삭제할 수 있습니다.
      </div>
    {/if}

    {#if error}
      <div class="bg-red-50 text-red-600 px-4 py-3 rounded-lg mb-6 border border-red-100 text-sm">{error}</div>
    {/if}
    
    <div class="bg-white rounded-xl shadow-sm border border-gray-200 overflow-hidden">
      <div class="grid grid-cols-12 gap-4 px-6 py-3 bg-gray-50 border-b border-gray-100 text-xs font-semibold text-gray-500 uppercase">
        <div class="col-span-1 flex justify-center items-center">
          <input 
            type="checkbox" 
            class="rounded border-gray-300 text-indigo-600 focus:ring-indigo-500"
            checked={selectableForAll.length > 0 && selectableForAll.every((i) => selectedIds.has(i.id))}
            onchange={toggleAll}
          />
        </div>
        <div class="col-span-6 sm:col-span-4">이름</div>
        <div class="col-span-2 text-right hidden sm:block">크기</div>
        <div class="col-span-2 text-right hidden sm:block min-w-0">생성일</div>
        <div class="col-span-5 sm:col-span-3 text-right">동작</div>
      </div>

      <div class="divide-y divide-gray-50">
        {#if displayItems.length === 0 && crews.length === 0 && !loading && !searchActive}
          <div class="p-16 text-center text-gray-400 font-medium">{trashMode ? '휴지통이 비어 있습니다.' : '폴더가 비어있습니다.'}</div>
        {/if}

        {#if searchActive && displayItems.length === 0 && !loading}
          <div class="p-16 text-center text-gray-400 font-medium">검색 결과가 없습니다.</div>
        {/if}

        {#if !currentFolderId && !searchActive}
          {#each crews as crew (crew.id)}
            <div class="grid grid-cols-12 gap-4 px-6 py-3.5 items-center hover:bg-indigo-50/30 group transition-colors">
              <div class="col-span-1 flex justify-center items-center">
                <div class="w-4"></div>
              </div>

              <div class="col-span-6 sm:col-span-4 flex items-center gap-3 overflow-hidden min-w-0">
                <div class="p-2 bg-amber-100 text-amber-600 rounded-lg shrink-0 group-hover:bg-amber-200 transition-colors">
                  <Users size={18} />
                </div>
                <button
                  onclick={() => enterCrew(crew)}
                  class="truncate font-medium text-gray-700 hover:text-indigo-600 text-sm text-left transition-colors"
                >
                  {crew.name}
                </button>
                {#if crew.visibility === 'Private'}
                  <span class="inline-flex items-center gap-1 text-[10px] font-bold text-gray-500 bg-gray-100 px-1.5 py-0.5 rounded">
                    <Lock size={11} /> 비공개
                  </span>
                {:else}
                  <span class="inline-flex items-center gap-1 text-[10px] font-bold text-emerald-600 bg-emerald-50 px-1.5 py-0.5 rounded">
                    <Globe size={11} /> 공개
                  </span>
                {/if}
                {#if !crew.is_member}
                  <span class="text-[10px] font-medium text-gray-400">미가입</span>
                {/if}
              </div>

              <div class="col-span-2 text-right text-xs text-gray-500 font-mono hidden sm:block">크루</div>

              <div class="col-span-2 text-right text-xs text-gray-400 hidden sm:block">-</div>

              <div class="col-span-5 sm:col-span-3 flex justify-end items-center shrink-0">
                {#if !crew.is_member}
                  <button
                    onclick={() => handleJoinCrew(crew)}
                    class="p-1.5 text-gray-400 hover:text-indigo-600 transition-colors"
                    title="가입 신청"
                  >
                    <UserPlus size={16} />
                  </button>
                {/if}
              </div>
            </div>
          {/each}
        {/if}

        {#each displayItems as item (item.id)}
          <div class="grid grid-cols-12 gap-4 px-6 py-3.5 items-center hover:bg-indigo-50/30 group transition-colors">
            <div class="col-span-1 flex justify-center items-center">
              {#if !item.is_dir || trashMode}
                <input 
                  type="checkbox" 
                  class="rounded border-gray-300 text-indigo-600 focus:ring-indigo-500"
                  checked={selectedIds.has(item.id)}
                  onchange={() => toggleSelection(item.id)}
                />
              {:else}
                <div class="w-4"></div>
              {/if}
            </div>

            <div class="col-span-6 sm:col-span-4 flex items-center gap-3 overflow-hidden min-w-0">
              {#if item.is_dir}
                <div class="p-2 bg-indigo-100 text-indigo-600 rounded-lg shrink-0 group-hover:bg-indigo-200 transition-colors">
                  <Folder size={18} fill="currentColor" />
                </div>
              {:else}
                {@const fileInfo = getFileInfo(item.name)}
                <div class="p-2 {fileInfo.color} rounded-lg shrink-0 group-hover:bg-white transition-colors">
                  <fileInfo.icon size={18} />
                </div>
              {/if}
              <div class="min-w-0 flex-1">
                <button 
                  onclick={() => openItem(item)}
                  class="truncate font-medium text-gray-700 hover:text-indigo-600 text-sm text-left transition-colors block w-full"
                >
                  {item.name}
                </button>
                {#if searchActive && item.path}
                  <p class="text-[10px] text-gray-400 truncate">{item.path}</p>
                {/if}
              </div>
            </div>

            <div class="col-span-2 text-right text-xs text-gray-500 font-mono hidden sm:block">
              {formatSize(item.size)}
            </div>

            <div class="col-span-2 text-right text-xs text-gray-400 hidden sm:block truncate min-w-0 px-1">
              {item.created_at ? format(new Date(item.created_at), 'yyyy-MM-dd HH:mm') : '-'}
            </div>

            <div class="col-span-5 sm:col-span-3 flex justify-end items-center gap-0.5 shrink-0 pl-1">
              {#if trashMode && canWrite}
                <button onclick={() => handleRestore(item)} class="p-1 text-gray-400 hover:text-emerald-600 transition-colors shrink-0" title="복구">
                  <RotateCcw size={16} />
                </button>
                <button onclick={() => handlePermanentDelete(item)} class="p-1 text-gray-400 hover:text-red-600 transition-colors shrink-0" title="영구 삭제">
                  <XCircle size={16} />
                </button>
              {:else}
                {#if !item.is_dir && isPreviewable(item)}
                  <button
                    onclick={() => openPreview(item)}
                    class="p-1 text-gray-400 hover:text-purple-600 transition-colors shrink-0"
                    title="미리보기"
                  >
                    <Eye size={16} />
                  </button>
                {/if}
                {#if !item.is_dir}
                  <button 
                    onclick={() => handleDownload(item.id)} 
                    class="p-1 text-gray-400 hover:text-indigo-600 transition-colors shrink-0"
                    title="다운로드"
                  >
                    <Download size={16} />
                  </button>
                {/if}
                {#if canWrite && !searchActive}
                  <button onclick={() => handleRename(item)} class="p-1 text-gray-400 hover:text-indigo-600 transition-colors shrink-0" title="이름 변경">
                    <Pencil size={16} />
                  </button>
                  <button onclick={() => handleMove(item)} class="p-1 text-gray-400 hover:text-indigo-600 transition-colors shrink-0" title="현재 폴더로 이동">
                    <FolderInput size={16} />
                  </button>
                  <button 
                    onclick={() => handleDelete(item)} 
                    class="p-1 text-gray-400 hover:text-red-600 transition-colors shrink-0"
                    title="삭제"
                  >
                    <Trash2 size={16} />
                  </button>
                {/if}
              {/if}
            </div>
          </div>
        {/each}
      </div>
    </div>
  </main>

  {#if previewItem}
    <div
      bind:this={previewShell}
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/90"
      class:!bg-black={previewFullscreen}
      role="dialog"
      aria-modal="true"
      aria-label="파일 미리보기"
      onclick={closePreview}
      tabindex="-1"
    >
      <div
        class="relative w-full h-full max-w-[100vw] max-h-[100vh] flex flex-col items-center justify-center"
        class:p-4={!previewFullscreen || !isImagePreview(previewItem)}
        onclick={(e) => e.stopPropagation()}
        role="presentation"
      >
        <div class="absolute top-4 right-4 flex items-center gap-2 z-10">
          {#if isImagePreview(previewItem)}
            <button
              onclick={togglePreviewFullscreen}
              class="p-2 rounded-full bg-black/50 text-white hover:bg-black/70"
              title="전체화면 (F)"
            >
              {#if previewFullscreen}
                <Minimize size={18} />
              {:else}
                <Maximize size={18} />
              {/if}
            </button>
          {/if}
          <button
            onclick={closePreview}
            class="p-2 rounded-full bg-black/50 text-white hover:bg-black/70 flex items-center gap-1 text-sm"
            title="닫기 (Esc)"
          >
            <X size={18} />
          </button>
        </div>

        {#if previewFullscreen && isImagePreview(previewItem) && imagePreviewItems.length > 1 && previewImageUrl}
          <div class="absolute top-4 left-4 z-10 px-3 py-1.5 rounded-full bg-black/50 text-gray-200 text-xs font-medium tabular-nums">
            {previewImageIndex + 1} / {imagePreviewItems.length}
          </div>
        {/if}

        {#if isImagePreview(previewItem) && canPreviewPrev}
          <button
            onclick={() => navigatePreview(-1)}
            class="absolute left-2 md:left-6 top-1/2 -translate-y-1/2 p-3 rounded-full bg-black/50 text-white hover:bg-black/70 z-10"
            title="이전 (←)"
            aria-label="이전 이미지"
          >
            <ChevronLeft size={28} />
          </button>
        {/if}

        {#if isImagePreview(previewItem) && canPreviewNext}
          <button
            onclick={() => navigatePreview(1)}
            class="absolute right-2 md:right-6 top-1/2 -translate-y-1/2 p-3 rounded-full bg-black/50 text-white hover:bg-black/70 z-10"
            title="다음 (→)"
            aria-label="다음 이미지"
          >
            <ChevronRight size={28} />
          </button>
        {/if}

        {#if isVideoPreview(previewItem)}
          {#key `${previewItem.id}:${subtitleTracks.length}`}
            <!-- svelte-ignore a11y_media_has_caption -->
            <video
              bind:this={previewVideoEl}
              src={streamUrlFor(previewItem)}
              controls
              class="w-full max-h-[85vh] rounded-lg bg-black"
              crossorigin="use-credentials"
              tabindex="-1"
            >
              {#each subtitleTracks as track, i}
                <track
                  kind="subtitles"
                  src={track.src}
                  label={track.label}
                  srclang="ko"
                  default={i === 0}
                />
              {/each}
            </video>
          {/key}
          <p class="text-gray-400 text-center mt-2 text-xs">
            ← → 10초 이동 · Space 재생/일시정지
            {#if subtitleTracks.length > 0}
              · 자막 {subtitleTracks.length}개 (플레이어에서 선택)
            {/if}
          </p>
        {:else if isImagePreview(previewItem)}
          {#if previewImageLoading}
            <div class="flex flex-col items-center gap-3 text-gray-300">
              <RefreshCw size={32} class="animate-spin" />
              <p class="text-sm">이미지 불러오는 중...</p>
            </div>
          {:else if previewImageError}
            <p class="text-red-300 text-sm text-center px-4">{previewImageError}</p>
          {:else if previewImageUrl}
            <div
              class="flex items-center justify-center w-full"
              class:flex-1={previewFullscreen}
              class:min-h-0={previewFullscreen}
            >
              {#key previewImageUrl}
                <img
                  src={previewImageUrl}
                  alt={previewItem.name}
                  class="object-contain select-none max-w-full"
                  class:max-h-[85vh]={!previewFullscreen}
                  class:rounded-lg={!previewFullscreen}
                  class:h-full={previewFullscreen}
                  class:w-auto={previewFullscreen}
                  draggable="false"
                />
              {/key}
            </div>
          {/if}
          {#if imagePreviewItems.length > 1 && previewImageUrl && !previewFullscreen}
            <p class="text-gray-400 text-center mt-2 text-xs tabular-nums">
              {previewImageIndex + 1} / {imagePreviewItems.length}
            </p>
          {/if}
        {:else}
          <p class="text-gray-300 text-sm">미리보기를 지원하지 않는 형식입니다.</p>
        {/if}
        {#if !previewFullscreen}
          <p class="text-white text-center mt-3 text-sm truncate max-w-3xl px-4">
            {previewItem.name}
          </p>
        {/if}
      </div>
    </div>
  {/if}

 {#if !(previewFullscreen && previewItem && isImagePreview(previewItem))}
  <StorageWidget onCrewCreated={refreshAll} />
 {/if}
</div>
