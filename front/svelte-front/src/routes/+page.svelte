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
  import { NAS_URL } from '$lib/constants';
  import { 
    Folder, File, Upload, Home, ArrowLeft, Trash2, 
    FileVideo, FileAudio, FileImage, FileText, FileCode, 
    FileArchive, FileSearch, Download, HardDrive, RefreshCw, 
    FolderPlus, FileSpreadsheet, FilePieChart, FileType2, FileType, FileJson,
    Captions, FileChartColumn
  } from 'lucide-svelte';

  const API_BASE = NAS_URL;

  interface FileItem {
    id: string;
    name: string;
    is_dir: boolean;
    size: number;
    created_at: string;
  }

  // --- Svelte 5 Runes (상태 관리) ---
  let currentFolderId = $state<string | null>(null);
  let folderHistory = $state<{id: string | null, name: string}[]>([]);
  let items = $state<FileItem[]>([]);
  let loading = $state(false);
  let uploading = $state(false);
  let error = $state<string | null>(null);
  let uploadProgress = $state(0);
  let currentFileIndex = $state(0);
  let totalFilesCount = $state(0);
  let currentUploadingFileName = $state('');
  
  // 💡 다중 선택을 위한 Set 상태
  let selectedIds = $state<Set<string>>(new Set());

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
    return { icon: File, color: 'bg-gray-100 text-gray-500' };
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
    const selectableItems = items.filter(item => !item.is_dir); 
    const nextIds = new Set<string>();
    
    if (selectedIds.size !== selectableItems.length && selectableItems.length > 0) {
      selectableItems.forEach(item => nextIds.add(item.id));
    }
    selectedIds = nextIds; // 새로운 Set 할당
  }

  // 다중 다운로드 실행 함수
  async function handleMultiDownload() {
    if (selectedIds.size === 0) return;
    try {
      loading = true; 
      const response = await axios.post(
        `${API_BASE}/api/files/download-zip`, 
        Array.from(selectedIds), 
        { 
            responseType: 'blob',
            headers: {
                'Content-Type': 'application/json'
            }
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
    selectedIds = new Set(); // 💡 폴더 이동 시 선택 초기화
    loading = true;
    error = null;
    try {
      const url = folderId 
        ? `${API_BASE}/api/files?folder_id=${folderId}` 
        : `${API_BASE}/api/files`;
      
      const response = await axios.get<FileItem[]>(url);
      items = response.data.sort((a, b) => {
        if (a.is_dir === b.is_dir) return a.name.localeCompare(b.name);
        return a.is_dir ? -1 : 1;
      });
    } catch (err) {
      error = '서버 연결 실패. 백엔드가 실행 중인지 확인하세요.';
    } finally {
      loading = false;
    }
  }

  async function refreshAll() {
    try {
      await Promise.all([
        fetchFiles(currentFolderId),
        storageState.refresh() 
      ]);
    } catch (err) {
      console.error("데이터 갱신 중 오류 발생:", err);
    }
  }

  $effect(() => {
    fetchFiles(currentFolderId);
  });

  async function handleCreateFolder() {
    const folderName = prompt('새 폴더 이름을 입력하세요:', '새 폴더');
    if (!folderName) return;

    try {
      await axios.post(`${API_BASE}/api/folders`, {
        name: folderName.trim(),
        parent_id: currentFolderId
      });
      fetchFiles(currentFolderId);
    } catch {
      alert('폴더 생성 실패');
    }
  }

  async function handleUpload(event: Event) {
    const input = event.target as HTMLInputElement;
    if (!input.files?.length) return;
    
    uploading = true;
    const files = Array.from(input.files);
    totalFilesCount = files.length;
    let successCount = 0;

    for (let i = 0; i < files.length; i++){
      currentFileIndex = i;
      currentUploadingFileName = files[i].name;
      try {
        const url = `${API_BASE}/api/upload/${encodeURIComponent(files[i].name)}${currentFolderId ? `?folder_id=${currentFolderId}` : ''}`;

        await axios.post(url, files[i], {
          headers: { 
            'Content-Type': 'application/octet-stream',
            'X-File-Size': files[i].size.toString() 
          },
          onUploadProgress: (p) => {
            const total = p.total || files[i].size;
            uploadProgress = Math.round((p.loaded * 100) / total);
          }
        });
        successCount++;
      } 
      catch (err) {
        console.error(`${files[i].name} 업로드 실패:`, err);
      }
    }  
    uploading = false;
    uploadProgress = 0;
    currentFileIndex = 0;
    input.value = '';
    await refreshAll();

    if (successCount > 0) {
      alert(`${successCount}개의 파일이 성공적으로 업로드 되었습니다!`);
    } else {
      alert('업로드에 실패했습니다.');
    }
  }

  async function handleDelete(item: FileItem) {
    const typeName = item.is_dir ? '폴더' : '파일';
    if (!confirm(`정말로 이 ${typeName}('${item.name}')을(를) 삭제하시겠습니까?`)) return;

    try {
      const endpoint = item.is_dir ? 'folders' : 'files';
      await axios.delete(`${API_BASE}/api/${endpoint}/${item.id}`);
      fetchFiles(currentFolderId);
    } catch {
      alert(`${typeName} 삭제 실패`);
    }
  }

  async function handleEmptyTrash() {
    if (!confirm('휴지통을 비우시겠습니까?\n이 작업은 되돌릴 수 없으며 모든 데이터가 영구 삭제됩니다.')) return;

    loading = true;
    try {
      const response = await axios.post(`${API_BASE}/api/nas/empty-trash`);
      if (response.data.success) {
        alert(response.data.message);
        refreshAll();
      }
    } catch {
      alert('휴지통 비우기 실패');
    } finally {
      loading = false;
    }
  }

  function handleDownload(id: string) {
    window.open(`${API_BASE}/api/files/${id}`, '_blank');
  }

  const enterFolder = (id: string, name: string) => {
    folderHistory = [...folderHistory, { id: currentFolderId, name }];
    currentFolderId = id;
  };

  const goUp = () => {
    const previous = folderHistory.pop();
    if (previous !== undefined) currentFolderId = previous.id;
  };

  const goToHome = () => {
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
      <button 
        onclick={() => fetchFiles(currentFolderId)} 
        class="p-2 text-gray-500 hover:bg-gray-100 rounded-full transition-colors"
        title="새로고침"
      >
        <RefreshCw size={20} class={loading ? "animate-spin text-indigo-600" : ""} />
      </button>

      <button 
        onclick={handleEmptyTrash} 
        disabled={loading}
        class="flex items-center gap-2 px-3 py-2 bg-white border border-red-200 text-red-600 rounded-lg hover:bg-red-50 transition-colors shadow-sm text-sm font-medium disabled:opacity-50"
      >
        <Trash2 size={18} class="text-red-500" />
        <span class="hidden sm:inline">휴지통 비우기</span>
      </button>

      <button 
        onclick={handleCreateFolder} 
        class="flex items-center gap-2 px-3 py-2 bg-white border border-gray-200 text-gray-700 rounded-lg hover:bg-gray-50 transition-colors shadow-sm text-sm font-medium"
      >
        <FolderPlus size={18} class="text-indigo-500" />
        <span>새 폴더</span>
      </button>

      {#if selectedIds.size > 0}
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

      <label class="relative flex items-center gap-2 px-4 py-2 bg-gray-100 text-indigo-700 rounded-lg cursor-pointer hover:bg-gray-200 transition-colors shadow-sm overflow-hidden min-w-[100px] justify-center">
        <Upload size={18} />
        <span class="font-medium text-sm">업로드</span>
        <input type="file" class="hidden" onchange={handleUpload} disabled={uploading} multiple />
      </label>
    </div>
  </header>

  <main class="p-4 md:p-6 max-w-7xl mx-auto">
    {#if currentFolderId}
      <div class="mb-4">
        <button onclick={goUp} class="flex items-center gap-2 text-gray-600 hover:text-indigo-700 bg-white border border-gray-200 px-3 py-1.5 rounded-lg shadow-sm text-sm font-medium">
          <ArrowLeft size={16} />
          <span>상위 폴더</span>
        </button>
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
            checked={items.length > 0 && items.filter(i => !i.is_dir).every(i => selectedIds.has(i.id))}
            onchange={toggleAll}
          />
        </div>
        <div class="col-span-5">이름</div>
        <div class="col-span-2 text-right hidden sm:block">크기</div>
        <div class="col-span-3 text-right hidden sm:block">생성일</div>
        <div class="col-span-1 text-right pr-2">동작</div>
      </div>

      <div class="divide-y divide-gray-50">
        {#if items.length === 0 && !loading}
          <div class="p-16 text-center text-gray-400 font-medium">폴더가 비어있습니다.</div>
        {/if}
        
        {#each items as item (item.id)}
          <div class="grid grid-cols-12 gap-4 px-6 py-3.5 items-center hover:bg-indigo-50/30 group transition-colors">
            <div class="col-span-1 flex justify-center items-center">
              {#if !item.is_dir}
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

            <div class="col-span-5 flex items-center gap-3 overflow-hidden">
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
              <button 
                onclick={() => item.is_dir ? enterFolder(item.id, item.name) : handleDownload(item.id)}
                class="truncate font-medium text-gray-700 hover:text-indigo-600 text-sm text-left transition-colors"
              >
                {item.name}
              </button>
            </div>

            <div class="col-span-2 text-right text-xs text-gray-500 font-mono hidden sm:block">
              {formatSize(item.size)}
            </div>

            <div class="col-span-3 text-right text-xs text-gray-400 hidden sm:block">
              {item.created_at ? format(new Date(item.created_at), 'yyyy-MM-dd HH:mm') : '-'}
            </div>

            <div class="col-span-1 flex justify-end items-center gap-1">
              {#if !item.is_dir}
                <button 
                  onclick={() => handleDownload(item.id)} 
                  class="p-1.5 text-gray-400 hover:text-indigo-600 transition-colors"
                  title="다운로드"
                >
                  <Download size={16} />
                </button>
              {/if}
              <button 
                onclick={() => handleDelete(item)} 
                class="p-1.5 text-gray-400 hover:text-red-600 transition-colors"
                title="삭제"
              >
                <Trash2 size={16} />
              </button>
            </div>
          </div>
        {/each}
      </div>
    </div>
  </main>
 <StorageWidget />
</div>
