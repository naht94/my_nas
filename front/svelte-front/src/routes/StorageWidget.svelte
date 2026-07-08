<script>
  import { goto } from '$app/navigation';
  import { base } from '$app/paths';
  import { storageState } from '$lib/states/storage.svelte.js';
  import { getNasUrl, GLOBAL_ROOT_CREW_ID } from '$lib/constants';
  import { Settings, X, Activity, HardDrive, User, ChevronRight, Users, Globe, Lock } from 'lucide-svelte';
  import { fly } from 'svelte/transition';

  let { onCrewCreated = () => {} } = $props();

  let isMenuOpen = $state(false);
  let isStorageOpen = $state(false);
  let isCrewOpen = $state(false);
  let crewName = $state('');
  let crewVisibility = $state('public');
  let crewError = $state('');
  let crewSubmitting = $state(false);

  const s = $derived(storageState.current);
  const usagePercent = $derived(s.total > 0 ? ((s.used / s.total) * 100).toFixed(1) : 0);
  const totalTB = $derived((s.total / Math.pow(1024, 4)).toFixed(2));
  const usedGB = $derived((s.used / Math.pow(1024, 3)).toFixed(1));

  function toggleMenu() {
    isMenuOpen = !isMenuOpen;
    if (!isMenuOpen) {
      isStorageOpen = false;
      isCrewOpen = false;
    }
  }

  function openStorage() {
    isCrewOpen = false;
    isStorageOpen = true;
    storageState.refresh();
  }

  function closeStorage() {
    isStorageOpen = false;
  }

  function openCrewForm() {
    isStorageOpen = false;
    isCrewOpen = true;
    crewError = '';
  }

  function closeCrewForm() {
    isCrewOpen = false;
  }

  async function createCrew() {
    crewError = '';
    const name = crewName.trim();
    if (!name) {
      crewError = '크루 이름을 입력하세요.';
      return;
    }
    crewSubmitting = true;
    try {
      const res = await fetch(`${getNasUrl()}/api/crews`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          parent_crew_id: GLOBAL_ROOT_CREW_ID,
          name,
          visibility: crewVisibility
        })
      });

      if (res.status === 401) {
        crewError = '로그인이 필요합니다. 먼저 로그인해주세요.';
        return;
      }
      if (!res.ok) {
        let msg = '크루 생성에 실패했습니다.';
        try {
          const data = await res.json();
          if (data?.message) msg = data.message;
        } catch (_) {}
        crewError = msg;
        return;
      }

      crewName = '';
      crewVisibility = 'public';
      isCrewOpen = false;
      isMenuOpen = false;
      onCrewCreated();
    } catch (e) {
      crewError = '서버와 통신할 수 없습니다.';
    } finally {
      crewSubmitting = false;
    }
  }

  function goToLogin() {
    isMenuOpen = false;
    isStorageOpen = false;
    isCrewOpen = false;
    goto(`${base}/login`);
  }
</script>

<div class="fixed bottom-6 right-6 z-50 flex flex-col items-end gap-3">
  {#if isStorageOpen}
    <div
      transition:fly={{ y: 20, duration: 300 }}
      class="w-72 bg-white rounded-2xl shadow-2xl border border-gray-100 p-5 overflow-hidden"
    >
      <div class="flex justify-between items-center mb-4">
        <div class="flex items-center gap-2 text-indigo-600">
          <Activity size={18} />
          <span class="text-sm font-bold tracking-tight">NAS 용량</span>
        </div>
        <button onclick={closeStorage} class="text-gray-400 hover:text-gray-600" title="닫기">
          <X size={18} />
        </button>
      </div>

      <div class="space-y-4">
        <div>
          <div class="flex justify-between text-[10px] font-bold text-gray-400 mb-1 tracking-wider">
            <span>STORAGE</span>
            <span class="text-indigo-600">{usagePercent}%</span>
          </div>
          <div class="w-full bg-gray-100 h-2.5 rounded-full overflow-hidden">
            <div
              class="h-full bg-indigo-500 transition-all duration-1000"
              style="width: {usagePercent}%"
            ></div>
          </div>
        </div>

        <div class="grid grid-cols-2 gap-2">
          <div class="bg-gray-50 p-2.5 rounded-xl">
            <p class="text-[9px] text-gray-400 font-bold uppercase">Used</p>
            <p class="text-sm font-black text-gray-700">{usedGB} GB</p>
          </div>
          <div class="bg-gray-50 p-2.5 rounded-xl">
            <p class="text-[9px] text-gray-400 font-bold uppercase">Total</p>
            <p class="text-sm font-black text-gray-700">{totalTB} TiB</p>
          </div>
        </div>
      </div>
    </div>
  {/if}

  {#if isCrewOpen}
    <div
      transition:fly={{ y: 20, duration: 300 }}
      class="w-72 bg-white rounded-2xl shadow-2xl border border-gray-100 p-5 overflow-hidden"
    >
      <div class="flex justify-between items-center mb-4">
        <div class="flex items-center gap-2 text-indigo-600">
          <Users size={18} />
          <span class="text-sm font-bold tracking-tight">크루 만들기</span>
        </div>
        <button onclick={closeCrewForm} class="text-gray-400 hover:text-gray-600" title="닫기">
          <X size={18} />
        </button>
      </div>

      <div class="space-y-4">
        <div>
          <label for="crew-name" class="block text-[11px] font-bold text-gray-400 mb-1 tracking-wider uppercase">이름</label>
          <input
            id="crew-name"
            type="text"
            bind:value={crewName}
            placeholder="크루 이름"
            onkeydown={(e) => { if (e.key === 'Enter') createCrew(); }}
            class="w-full px-3 py-2 border border-gray-200 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
          />
        </div>

        <div>
          <span class="block text-[11px] font-bold text-gray-400 mb-1.5 tracking-wider uppercase">공개 범위</span>
          <div class="grid grid-cols-2 gap-2">
            <button
              type="button"
              onclick={() => (crewVisibility = 'public')}
              class="flex items-center gap-2 px-3 py-2 rounded-lg border text-sm font-medium transition-colors {crewVisibility === 'public' ? 'border-indigo-500 bg-indigo-50 text-indigo-700' : 'border-gray-200 text-gray-600 hover:bg-gray-50'}"
            >
              <Globe size={16} /> 공개
            </button>
            <button
              type="button"
              onclick={() => (crewVisibility = 'private')}
              class="flex items-center gap-2 px-3 py-2 rounded-lg border text-sm font-medium transition-colors {crewVisibility === 'private' ? 'border-indigo-500 bg-indigo-50 text-indigo-700' : 'border-gray-200 text-gray-600 hover:bg-gray-50'}"
            >
              <Lock size={16} /> 비공개
            </button>
          </div>
          <p class="text-[11px] text-gray-400 mt-1.5">
            {crewVisibility === 'public'
              ? '누구나 크루를 보고 가입 신청할 수 있습니다.'
              : '초대받은 멤버만 크루를 보고 접근할 수 있습니다.'}
          </p>
        </div>

        {#if crewError}
          <p class="text-xs text-red-600 bg-red-50 px-3 py-2 rounded-lg">{crewError}</p>
        {/if}

        <button
          onclick={createCrew}
          disabled={crewSubmitting}
          class="w-full py-2.5 bg-indigo-600 text-white text-sm font-semibold rounded-lg hover:bg-indigo-700 transition-colors disabled:opacity-50"
        >
          {crewSubmitting ? '생성 중...' : '크루 생성'}
        </button>
      </div>
    </div>
  {/if}

  {#if isMenuOpen}
    <div
      transition:fly={{ y: 12, duration: 200 }}
      class="w-56 bg-white rounded-xl shadow-2xl border border-gray-100 py-2 overflow-hidden"
    >
      <button
        onclick={openStorage}
        class="w-full flex items-center gap-3 px-4 py-3 text-sm text-gray-700 hover:bg-indigo-50 hover:text-indigo-700 transition-colors"
      >
        <HardDrive size={18} class="text-indigo-500 shrink-0" />
        <span class="flex-1 text-left font-medium">NAS 용량 확인</span>
        <ChevronRight size={16} class="text-gray-300" />
      </button>
      <button
        onclick={openCrewForm}
        class="w-full flex items-center gap-3 px-4 py-3 text-sm text-gray-700 hover:bg-indigo-50 hover:text-indigo-700 transition-colors border-t border-gray-50"
      >
        <Users size={18} class="text-indigo-500 shrink-0" />
        <span class="flex-1 text-left font-medium">크루 만들기</span>
        <ChevronRight size={16} class="text-gray-300" />
      </button>
      <button
        onclick={goToLogin}
        class="w-full flex items-center gap-3 px-4 py-3 text-sm text-gray-700 hover:bg-indigo-50 hover:text-indigo-700 transition-colors border-t border-gray-50"
      >
        <User size={18} class="text-indigo-500 shrink-0" />
        <span class="flex-1 text-left font-medium">유저 설정</span>
        <ChevronRight size={16} class="text-gray-300" />
      </button>
    </div>
  {/if}

  <button
    onclick={toggleMenu}
    class="w-14 h-14 bg-indigo-600 text-white rounded-full shadow-lg hover:bg-indigo-700 hover:scale-110 transition-all flex items-center justify-center group relative"
    title="옵션"
  >
    {#if isMenuOpen}
      <X size={24} />
    {:else}
      <Settings size={24} />
      {#if Number(usagePercent) > 90}
        <span class="absolute top-0 right-0 w-4 h-4 bg-red-500 border-2 border-white rounded-full"></span>
      {/if}
    {/if}
  </button>
</div>
