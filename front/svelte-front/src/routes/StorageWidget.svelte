<script>
  import { storageState } from '$lib/states/storage.svelte.js';
  import { HardDrive, X, Activity } from 'lucide-svelte';
  import { fade, fly } from 'svelte/transition';

  let isOpen = $state(false); // 팝업 열림 상태

  const s = $derived(storageState.current);
  const usagePercent = $derived(s.total > 0 ? ((s.used / s.total) * 100).toFixed(1) : 0);
  const totalTB = $derived((s.total / Math.pow(1024, 4)).toFixed(2));
  const usedGB = $derived((s.used / Math.pow(1024, 3)).toFixed(1));

  function toggleDashboard() {
    isOpen = !isOpen;
    if (isOpen) storageState.refresh(); // 열 때마다 최신화
  }
</script>

<div class="fixed bottom-6 right-6 z-50 flex flex-col items-end gap-4">
  
  {#if isOpen}
    <div 
      transition:fly={{ y: 20, duration: 300 }}
      class="w-72 bg-white rounded-2xl shadow-2xl border border-gray-100 p-5 mb-2 overflow-hidden"
    >
      <div class="flex justify-between items-center mb-4">
        <div class="flex items-center gap-2 text-indigo-600">
          <Activity size={18} />
          <span class="text-sm font-bold tracking-tight">System Status</span>
        </div>
        <button onclick={toggleDashboard} class="text-gray-400 hover:text-gray-600">
          <X size={18} />
        </button>
      </div>

      <div class="space-y-4">
        <div>
          <div class="flex justify-between text-[10px] font-bold text-gray-400 mb-1 tracking-wider">
            <span>STORAGE (8TB HDD)</span>
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

  <button 
    onclick={toggleDashboard}
    class="w-14 h-14 bg-indigo-600 text-white rounded-full shadow-lg hover:bg-indigo-700 hover:scale-110 transition-all flex items-center justify-center group relative"
  >
    {#if isOpen}
      <X size={24} />
    {:else}
      <HardDrive size={24} />
      {#if Number(usagePercent) > 90}
        <span class="absolute top-0 right-0 w-4 h-4 bg-red-500 border-2 border-white rounded-full"></span>
      {/if}
    {/if}
  </button>
</div>
