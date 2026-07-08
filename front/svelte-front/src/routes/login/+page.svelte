<script>
  import { goto } from '$app/navigation';
  import { base } from '$app/paths';
  import { onMount } from 'svelte';
  import { getNasUrl, GLOBAL_ROOT_CREW_ID } from '$lib/constants';
  import { authState } from '$lib/states/auth.svelte.js';

  let username = '';
  let password = '';
  let errorMessage = '';
  let webdavMounts = $state([]);
  let loadingMounts = $state(false);

  // 앱 비밀번호(WebDAV 전용) 상태
  let appPasswords = $state([]);
  let newAppPasswordLabel = '';
  let issuedAppPassword = $state('');
  let appPwError = $state('');

  // 크루 관리 상태
  let manageableCrews = $state([]);
  let deletableCrews = $state([]);
  let deleteTargetCrewId = $state('');
  let selectedCrewId = $state('');
  let crewMembers = $state([]);
  let inviteUsername = $state('');
  let inviteRole = $state(2); // 2 = Member, 1 = Manager
  let manageError = $state('');

  // 전역(글로벌) 설정 상태
  let globalDepth = $state(null);
  let globalDepthInput = $state(0);
  let globalSettingsError = $state('');
  let canManageGlobal = $state(false);
  let globalPendingMembers = $state([]);
  let globalMembershipPending = $state(false);

  // 비밀번호 변경
  let currentPassword = $state('');
  let newPassword = $state('');
  let passwordChangeError = $state('');
  let passwordChangeSuccess = $state('');

  // 로그인 세션 관리
  let sessions = $state([]);
  let sessionsError = $state('');
  let sessionsLoading = $state(false);
  let globalActiveMembers = $state([]);

  // 감사 로그 (글로벌 관리자)
  let auditLogs = $state([]);
  let auditLogsLoading = $state(false);
  let auditLogsError = $state('');

  const ROLE_LABELS = { 0: 'Owner', 1: 'Manager', 2: 'Member', 3: 'Guest' };
  const STATUS_LABELS = { 0: '가입대기', 1: '활성', 2: '초대됨', 3: '차단됨' };

  /** 로그인 후 설정 패널: account | crew */
  let settingsPanel = $state('account');

  onMount(async () => {
    const ok = await authState.refresh();
    if (ok) {
      await syncMembershipStatus();
      await Promise.all([
        loadWebDavMounts(),
        loadAppPasswords(),
        loadManageableCrews(),
        loadDeletableCrews(),
        loadGlobalSettings(),
        loadSessions()
      ]);
    }
  });

  async function syncMembershipStatus() {
    if (!authState.isLoggedIn) return;
    try {
      const res = await fetch(`${getNasUrl()}/api/users/me`, { credentials: 'include' });
      if (res.ok) {
        const data = await res.json();
        globalMembershipPending = data.global_status === 0;
      }
    } catch {
      globalMembershipPending = false;
    }
  }

  async function handleLogin() {
    errorMessage = '';
    webdavMounts = [];

    if (!username || !password) {
      errorMessage = '아이디와 비밀번호를 입력해주세요.';
      return;
    }

    try {
      const response = await fetch(`${getNasUrl()}/api/users/login`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ username, password })
      });

      if (!response.ok) {
        errorMessage = '아이디 또는 비밀번호가 올바르지 않습니다.';
        return;
      }

      const data = await response.json();
      authState.setSession(data.user_id, data.username);
      globalMembershipPending = data.global_status === 0;
      username = '';
      password = '';
      await Promise.all([
        loadWebDavMounts(),
        loadAppPasswords(),
        loadManageableCrews(),
        loadDeletableCrews(),
        loadGlobalSettings(),
        loadSessions()
      ]);
    } catch (error) {
      console.error('API Error:', error);
      errorMessage = '서버와 통신할 수 없습니다.';
    }
  }

  async function loadWebDavMounts() {
    if (!authState.isLoggedIn) return;
    loadingMounts = true;
    try {
      const response = await fetch(`${getNasUrl()}/api/crews/webdav-mounts`, {
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' }
      });
      if (response.ok) {
        webdavMounts = await response.json();
      }
    } catch (error) {
      console.error('WebDAV mounts load failed:', error);
    } finally {
      loadingMounts = false;
    }
  }

  async function loadAppPasswords() {
    if (!authState.isLoggedIn) return;
    try {
      const response = await fetch(`${getNasUrl()}/api/users/app-passwords`, {
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' }
      });
      if (response.ok) {
        appPasswords = await response.json();
      }
    } catch (error) {
      console.error('app-passwords load failed:', error);
    }
  }

  async function createAppPassword() {
    appPwError = '';
    issuedAppPassword = '';
    try {
      const response = await fetch(`${getNasUrl()}/api/users/app-passwords`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ label: newAppPasswordLabel || null })
      });
      if (!response.ok) {
        appPwError = '앱 비밀번호 발급에 실패했습니다.';
        return;
      }
      const data = await response.json();
      issuedAppPassword = data.app_password;
      newAppPasswordLabel = '';
      await loadAppPasswords();
    } catch (error) {
      console.error('create app-password failed:', error);
      appPwError = '서버와 통신할 수 없습니다.';
    }
  }

  async function revokeAppPassword(id) {
    try {
      await fetch(`${getNasUrl()}/api/users/app-passwords/${id}`, {
        method: 'DELETE',
        credentials: 'include'
      });
      await loadAppPasswords();
    } catch (error) {
      console.error('revoke app-password failed:', error);
    }
  }

  async function loadDeletableCrews() {
    if (!authState.isLoggedIn) return;
    try {
      const res = await fetch(`${getNasUrl()}/api/crews/deletable`, { credentials: 'include' });
      if (res.ok) {
        deletableCrews = await res.json();
        if (deleteTargetCrewId && !deletableCrews.some((c) => c.id === deleteTargetCrewId)) {
          deleteTargetCrewId = '';
        }
      }
    } catch (error) {
      console.error('deletable crews load failed:', error);
    }
  }

  async function loadManageableCrews() {
    if (!authState.isLoggedIn) return;
    try {
      const res = await fetch(`${getNasUrl()}/api/crews/manageable`, { credentials: 'include' });
      if (res.ok) {
        manageableCrews = await res.json();
        if (selectedCrewId && !manageableCrews.some((c) => c.id === selectedCrewId)) {
          selectedCrewId = '';
          crewMembers = [];
        }
      }
    } catch (error) {
      console.error('manageable crews load failed:', error);
    }
  }

  async function selectCrew(crewId) {
    manageError = '';
    selectedCrewId = crewId;
    crewMembers = [];
    if (!crewId) return;
    try {
      const res = await fetch(`${getNasUrl()}/api/crews/${crewId}/members`, { credentials: 'include' });
      if (res.ok) {
        crewMembers = await res.json();
      } else {
        manageError = '멤버 목록을 불러오지 못했습니다.';
      }
    } catch (error) {
      manageError = '서버와 통신할 수 없습니다.';
    }
  }

  async function approveMember(userId) {
    manageError = '';
    try {
      const res = await fetch(`${getNasUrl()}/api/crews/${selectedCrewId}/approve`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ target_user_id: userId })
      });
      if (!res.ok) {
        manageError = '승인에 실패했습니다.';
        return;
      }
      await selectCrew(selectedCrewId);
    } catch (error) {
      manageError = '서버와 통신할 수 없습니다.';
    }
  }

  async function banMember(userId) {
    if (!confirm('이 멤버를 차단하시겠습니까? 파일 접근이 즉시 중단됩니다.')) return;
    manageError = '';
    try {
      const res = await fetch(`${getNasUrl()}/api/crews/${selectedCrewId}/ban`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ target_user_id: userId })
      });
      if (!res.ok) {
        const data = await res.json().catch(() => ({}));
        manageError = data.message || '차단에 실패했습니다.';
        return;
      }
      await selectCrew(selectedCrewId);
    } catch (error) {
      manageError = '서버와 통신할 수 없습니다.';
    }
  }

  async function deleteSelectedCrew() {
    const crew = deletableCrews.find((c) => c.id === deleteTargetCrewId);
    if (!crew) {
      manageError = '삭제할 Crew를 선택하세요.';
      return;
    }
    const via = crew.is_direct_owner ? '직접 Owner' : '상위 Crew Owner 권한';
    if (!confirm(
      `'${crew.name}' Crew를 삭제하시겠습니까? (${via})\n` +
        '하위 Crew·파일·폴더가 모두 영구 삭제되며 되돌릴 수 없습니다.'
    )) return;
    manageError = '';
    try {
      const res = await fetch(`${getNasUrl()}/api/crews/${deleteTargetCrewId}`, {
        method: 'DELETE',
        credentials: 'include'
      });
      if (!res.ok) {
        const data = await res.json().catch(() => ({}));
        manageError = data.error || data.message || 'Crew 삭제에 실패했습니다.';
        return;
      }
      deleteTargetCrewId = '';
      if (selectedCrewId && !manageableCrews.some((c) => c.id === selectedCrewId)) {
        selectedCrewId = '';
        crewMembers = [];
      }
      await Promise.all([loadManageableCrews(), loadDeletableCrews()]);
    } catch (error) {
      manageError = '서버와 통신할 수 없습니다.';
    }
  }

  async function changePassword() {
    passwordChangeError = '';
    passwordChangeSuccess = '';
    if (!currentPassword || !newPassword) {
      passwordChangeError = '현재 비밀번호와 새 비밀번호를 입력하세요.';
      return;
    }
    if (newPassword.length < 8) {
      passwordChangeError = '새 비밀번호는 8자 이상이어야 합니다.';
      return;
    }
    try {
      const res = await fetch(`${getNasUrl()}/api/users/change-password`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          current_password: currentPassword,
          new_password: newPassword
        })
      });
      if (!res.ok) {
        passwordChangeError = '비밀번호 변경에 실패했습니다. 현재 비밀번호를 확인하세요.';
        return;
      }
      passwordChangeSuccess = '비밀번호가 변경되었습니다.';
      currentPassword = '';
      newPassword = '';
    } catch (error) {
      passwordChangeError = '서버와 통신할 수 없습니다.';
    }
  }

  async function inviteMember() {
    manageError = '';
    const name = inviteUsername.trim();
    if (!name) {
      manageError = '초대할 아이디를 입력하세요.';
      return;
    }
    try {
      const res = await fetch(`${getNasUrl()}/api/crews/${selectedCrewId}/invite`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ username: name, role: Number(inviteRole) })
      });
      if (!res.ok) {
        let msg = '초대에 실패했습니다.';
        try {
          const d = await res.json();
          if (d?.error) msg = d.error;
          else if (d?.message) msg = d.message;
        } catch (_) {}
        manageError = msg;
        return;
      }
      inviteUsername = '';
      await selectCrew(selectedCrewId);
    } catch (error) {
      manageError = '서버와 통신할 수 없습니다.';
    }
  }

  async function loadGlobalSettings() {
    if (!authState.isLoggedIn) return;
    try {
      const res = await fetch(`${getNasUrl()}/api/crews/${GLOBAL_ROOT_CREW_ID}/settings`, {
        credentials: 'include'
      });
      if (res.ok) {
        const crew = await res.json();
        canManageGlobal = true;
        globalDepth = crew.max_sub_crew_depth;
        globalDepthInput = crew.max_sub_crew_depth;
        await loadGlobalPendingMembers();
        await loadAuditLogs();
      } else {
        canManageGlobal = false;
      }
    } catch (error) {
      canManageGlobal = false;
    }
  }

  async function loadSessions() {
    if (!authState.isLoggedIn) return;
    sessionsLoading = true;
    sessionsError = '';
    try {
      const res = await fetch(`${getNasUrl()}/api/users/sessions`, { credentials: 'include' });
      if (res.ok) {
        sessions = await res.json();
      } else {
        sessions = [];
        sessionsError = '세션 목록을 불러오지 못했습니다.';
      }
    } catch {
      sessions = [];
      sessionsError = '서버와 통신할 수 없습니다.';
    } finally {
      sessionsLoading = false;
    }
  }

  async function revokeOtherSessions() {
    if (!confirm('현재 기기를 제외한 다른 모든 로그인 세션을 종료하시겠습니까?')) return;
    sessionsError = '';
    try {
      const res = await fetch(`${getNasUrl()}/api/users/sessions/revoke-others`, {
        method: 'POST',
        credentials: 'include'
      });
      if (!res.ok) {
        sessionsError = '다른 세션 종료에 실패했습니다.';
        return;
      }
      const data = await res.json();
      alert(data.message || '다른 기기 세션을 종료했습니다.');
      await loadSessions();
    } catch {
      sessionsError = '서버와 통신할 수 없습니다.';
    }
  }

  async function loadAuditLogs() {
    if (!canManageGlobal) return;
    auditLogsLoading = true;
    auditLogsError = '';
    try {
      const res = await fetch(`${getNasUrl()}/api/audit-logs?limit=80`, {
        credentials: 'include'
      });
      if (res.ok) {
        auditLogs = await res.json();
      } else {
        auditLogs = [];
        auditLogsError = '감사 로그를 불러오지 못했습니다.';
      }
    } catch {
      auditLogs = [];
      auditLogsError = '서버와 통신할 수 없습니다.';
    } finally {
      auditLogsLoading = false;
    }
  }

  async function loadGlobalPendingMembers() {
    if (!canManageGlobal) return;
    try {
      const res = await fetch(`${getNasUrl()}/api/crews/${GLOBAL_ROOT_CREW_ID}/members`, {
        credentials: 'include'
      });
      if (res.ok) {
        const members = await res.json();
        globalPendingMembers = members.filter((m) => m.status === 0);
        globalActiveMembers = members.filter((m) => m.status === 1 && m.role !== 0);
      }
    } catch {
      globalPendingMembers = [];
      globalActiveMembers = [];
    }
  }

  async function banGlobalMember(userId) {
    if (!confirm('이 사용자를 글로벌 크루에서 차단하시겠습니까? 파일 접근이 즉시 중단됩니다.')) return;
    globalSettingsError = '';
    try {
      const res = await fetch(`${getNasUrl()}/api/crews/${GLOBAL_ROOT_CREW_ID}/ban`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ target_user_id: userId })
      });
      if (!res.ok) {
        const data = await res.json().catch(() => ({}));
        globalSettingsError = data.message || '차단에 실패했습니다.';
        return;
      }
      await loadGlobalPendingMembers();
    } catch {
      globalSettingsError = '서버와 통신할 수 없습니다.';
    }
  }

  async function approveGlobalMember(userId) {
    globalSettingsError = '';
    try {
      const res = await fetch(`${getNasUrl()}/api/crews/${GLOBAL_ROOT_CREW_ID}/approve`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ target_user_id: userId })
      });
      if (!res.ok) {
        globalSettingsError = '가입 승인에 실패했습니다.';
        return;
      }
      await loadGlobalPendingMembers();
    } catch {
      globalSettingsError = '서버와 통신할 수 없습니다.';
    }
  }

  async function saveGlobalDepth() {
    globalSettingsError = '';
    const v = Number(globalDepthInput);
    if (!Number.isInteger(v) || v < 0) {
      globalSettingsError = '0 이상의 정수를 입력하세요.';
      return;
    }
    try {
      const res = await fetch(`${getNasUrl()}/api/crews/${GLOBAL_ROOT_CREW_ID}/settings`, {
        method: 'PATCH',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ max_sub_crew_depth: v })
      });
      if (!res.ok) {
        globalSettingsError = '설정 저장에 실패했습니다.';
        return;
      }
      const crew = await res.json();
      globalDepth = crew.max_sub_crew_depth;
      globalDepthInput = crew.max_sub_crew_depth;
      alert('전역 depth 설정을 저장했습니다. 하위 크루의 depth 제약도 함께 조정되었습니다.');
    } catch (error) {
      globalSettingsError = '서버와 통신할 수 없습니다.';
    }
  }

  async function copyText(value, label) {
    try {
      await navigator.clipboard.writeText(value);
      alert(`${label}이(가) 복사되었습니다.`);
    } catch {
      prompt(`${label}을(를) 복사하세요:`, value);
    }
  }

  async function handleLogout() {
    await authState.logout();
    webdavMounts = [];
    appPasswords = [];
    issuedAppPassword = '';
    manageableCrews = [];
    deletableCrews = [];
    deleteTargetCrewId = '';
    selectedCrewId = '';
    crewMembers = [];
    auditLogs = [];
    settingsPanel = 'account';
    canManageGlobal = false;
    globalDepth = null;
    globalPendingMembers = [];
    globalMembershipPending = false;
  }

  async function copyMountUrl(url) {
    await copyText(url, 'WebDAV 주소');
  }

  function goToRegister() {
    goto(`${base}/register`);
  }

  function goToHome() {
    goto(`${base}/`);
  }
</script>

<main class="min-h-screen bg-gray-50 flex items-center justify-center p-4">
  <div class="w-full max-w-lg bg-white rounded-2xl shadow-lg border border-gray-100 p-8">
    <button
      type="button"
      onclick={goToHome}
      class="text-sm text-gray-500 hover:text-indigo-600 mb-6"
    >
      ← NAS 홈으로
    </button>

    <h2 class="text-2xl font-bold text-gray-800 mb-2">
      {#if authState.isLoggedIn}
        설정
      {:else}
        로그인
      {/if}
    </h2>
    <p class="text-sm text-gray-500 mb-8">
      {#if authState.isLoggedIn}
        회원 정보와 크루 관리를 탭에서 선택하세요.
      {:else}
        로그인 후 WebDAV·크루 설정을 이용할 수 있습니다.
      {/if}
    </p>

    {#if authState.isLoggedIn}
      {#if globalMembershipPending}
        <div class="mb-6 p-4 bg-amber-50 rounded-xl border border-amber-200">
          <p class="text-sm text-amber-800 font-medium">가입 승인 대기 중입니다.</p>
          <p class="text-xs text-amber-700 mt-1">글로벌 크루 관리자가 승인하면 NAS를 이용할 수 있습니다.</p>
        </div>
      {/if}
      <div class="mb-6 p-4 bg-indigo-50 rounded-xl border border-indigo-100">
        <p class="text-sm text-gray-600">로그인됨</p>
        <p class="text-lg font-bold text-indigo-700">{authState.username}</p>
        <p class="text-xs text-gray-500 mt-1">user_id: {authState.userId}</p>
        <button
          type="button"
          onclick={handleLogout}
          class="mt-3 text-sm text-red-600 hover:text-red-700 font-medium"
        >
          로그아웃
        </button>
      </div>

      <div class="flex gap-1 p-1 bg-gray-100 rounded-xl mb-8" role="tablist">
        <button
          type="button"
          role="tab"
          aria-selected={settingsPanel === 'account'}
          onclick={() => (settingsPanel = 'account')}
          class="flex-1 py-2.5 text-sm font-semibold rounded-lg transition-colors {settingsPanel === 'account'
            ? 'bg-white text-indigo-700 shadow-sm'
            : 'text-gray-500 hover:text-gray-700'}"
        >
          회원 정보
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={settingsPanel === 'crew'}
          onclick={() => (settingsPanel = 'crew')}
          class="flex-1 py-2.5 text-sm font-semibold rounded-lg transition-colors {settingsPanel === 'crew'
            ? 'bg-white text-indigo-700 shadow-sm'
            : 'text-gray-500 hover:text-gray-700'}"
        >
          크루 정보
        </button>
      </div>

      {#if settingsPanel === 'account'}
      <section class="space-y-3">
        <h3 class="text-sm font-bold text-gray-700 uppercase tracking-wide">비밀번호 변경</h3>
        <p class="text-xs text-gray-500 leading-relaxed">
          로그인 비밀번호를 변경합니다. WebDAV 앱 비밀번호와는 별개입니다.
        </p>
        <input
          type="password"
          bind:value={currentPassword}
          placeholder="현재 비밀번호"
          class="w-full px-3 py-2 border border-gray-200 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
        />
        <input
          type="password"
          bind:value={newPassword}
          placeholder="새 비밀번호 (8자 이상)"
          class="w-full px-3 py-2 border border-gray-200 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
        />
        {#if passwordChangeError}
          <p class="text-sm text-red-600 bg-red-50 px-3 py-2 rounded-lg">{passwordChangeError}</p>
        {/if}
        {#if passwordChangeSuccess}
          <p class="text-sm text-emerald-700 bg-emerald-50 px-3 py-2 rounded-lg">{passwordChangeSuccess}</p>
        {/if}
        <button
          type="button"
          onclick={changePassword}
          class="px-4 py-2 bg-gray-800 text-white text-sm font-semibold rounded-lg hover:bg-gray-900"
        >
          비밀번호 변경
        </button>
      </section>

      <section class="space-y-3 mt-8 pt-6 border-t border-gray-100">
        <h3 class="text-sm font-bold text-gray-700 uppercase tracking-wide">로그인 세션</h3>
        <p class="text-xs text-gray-500 leading-relaxed">
          현재 계정으로 로그인된 기기 목록입니다. 분실·유출이 의심되면 다른 세션을 모두 종료하세요.
        </p>
        {#if sessionsLoading}
          <p class="text-sm text-gray-400">불러오는 중...</p>
        {:else if sessions.length === 0}
          <p class="text-sm text-gray-500 bg-gray-50 p-3 rounded-lg">활성 세션이 없습니다.</p>
        {:else}
          <ul class="space-y-2">
            {#each sessions as s}
              <li class="flex items-center justify-between border border-gray-200 rounded-lg px-3 py-2">
                <div>
                  <p class="text-sm font-medium text-gray-800">
                    {s.label || '웹 브라우저'}
                    {#if s.is_current}
                      <span class="ml-1 text-[10px] font-bold text-emerald-600 bg-emerald-50 px-1.5 py-0.5 rounded">현재</span>
                    {/if}
                  </p>
                  <p class="text-[10px] text-gray-400">
                    생성 {s.created_at?.slice(0, 16)} · 만료 {s.expires_at?.slice(0, 16)}
                  </p>
                </div>
              </li>
            {/each}
          </ul>
        {/if}
        {#if sessionsError}
          <p class="text-sm text-red-600 bg-red-50 px-3 py-2 rounded-lg">{sessionsError}</p>
        {/if}
        {#if sessions.length > 1}
          <button
            type="button"
            onclick={revokeOtherSessions}
            class="px-4 py-2 border border-red-200 text-red-600 text-sm font-semibold rounded-lg hover:bg-red-50"
          >
            다른 기기 모두 로그아웃
          </button>
        {/if}
      </section>

      <section class="space-y-3 mt-8 pt-6 border-t border-gray-100">
        <h3 class="text-sm font-bold text-gray-700 uppercase tracking-wide">WebDAV 앱 비밀번호</h3>
        <p class="text-xs text-gray-500 leading-relaxed">
          Windows 탐색기 등 WebDAV 연결에는 <strong>계정 비밀번호 대신 앱 비밀번호</strong>를 사용합니다.
          유출 시 해당 비밀번호만 폐기하면 됩니다. 발급된 값은 한 번만 표시됩니다.
        </p>

        <div class="flex gap-2">
          <input
            type="text"
            bind:value={newAppPasswordLabel}
            placeholder="용도 (예: 집 PC)"
            class="flex-1 px-3 py-2 border border-gray-200 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
          />
          <button
            type="button"
            onclick={createAppPassword}
            class="px-4 py-2 bg-indigo-600 text-white text-sm font-semibold rounded-lg hover:bg-indigo-700"
          >
            발급
          </button>
        </div>

        {#if appPwError}
          <p class="text-sm text-red-600 bg-red-50 px-3 py-2 rounded-lg">{appPwError}</p>
        {/if}

        {#if issuedAppPassword}
          <div class="border border-amber-200 bg-amber-50 rounded-xl p-4 space-y-2">
            <p class="text-xs font-bold text-amber-700">새 앱 비밀번호 (지금만 표시됩니다)</p>
            <code class="block text-xs bg-white p-2 rounded break-all text-gray-800">{issuedAppPassword}</code>
            <button
              type="button"
              onclick={() => copyText(issuedAppPassword, '앱 비밀번호')}
              class="text-sm text-indigo-600 hover:text-indigo-800 font-medium"
            >
              복사
            </button>
          </div>
        {/if}

        {#if appPasswords.length > 0}
          <ul class="space-y-2">
            {#each appPasswords as ap}
              <li class="flex items-center justify-between border border-gray-200 rounded-lg px-3 py-2">
                <div>
                  <p class="text-sm font-medium text-gray-800">{ap.label || '(이름 없음)'}</p>
                  <p class="text-[10px] text-gray-400">
                    발급 {ap.created_at?.slice(0, 10)}
                    {#if ap.last_used_at}· 최근사용 {ap.last_used_at.slice(0, 10)}{/if}
                  </p>
                </div>
                <button
                  type="button"
                  onclick={() => revokeAppPassword(ap.id)}
                  class="text-sm text-red-600 hover:text-red-700 font-medium"
                >
                  폐기
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </section>
      {:else if settingsPanel === 'crew'}
      <section class="space-y-3">
        <h3 class="text-sm font-bold text-gray-700 uppercase tracking-wide">WebDAV 마운트 (Owner 전용)</h3>
        <p class="text-xs text-gray-500 leading-relaxed">
          Windows 네트워크 드라이브 연결 시 아래 주소와 <strong>회원 정보</strong> 탭의 앱 비밀번호를 사용하세요.
        </p>

        {#if loadingMounts}
          <p class="text-sm text-gray-400">불러오는 중...</p>
        {:else if webdavMounts.length === 0}
          <p class="text-sm text-gray-500 bg-gray-50 p-4 rounded-lg">
            Owner인 Crew가 없습니다. Crew를 생성하면 WebDAV 마운트 주소가 표시됩니다.
          </p>
        {:else}
          {#each webdavMounts as mount}
            <div class="border border-gray-200 rounded-xl p-4 space-y-2">
              <p class="font-semibold text-gray-800">{mount.crew_name}</p>
              <code class="block text-xs bg-gray-100 p-2 rounded break-all text-gray-700">{mount.mount_path}</code>
              <p class="text-[10px] text-gray-400 break-all">crew_id: {mount.crew_id}</p>
              <button
                type="button"
                onclick={() => copyMountUrl(mount.mount_path)}
                class="text-sm text-indigo-600 hover:text-indigo-800 font-medium"
              >
                주소 복사
              </button>
            </div>
          {/each}
        {/if}
      </section>

      <section class="space-y-3 mt-8 pt-6 border-t border-gray-100">
        <h3 class="text-sm font-bold text-gray-700 uppercase tracking-wide">크루 멤버 관리</h3>
        <p class="text-xs text-gray-500 leading-relaxed">
          내가 <strong>Owner/Manager</strong>인 크루의 멤버를 관리합니다. 가입 신청 승인, 아이디로 초대를 할 수 있습니다.
        </p>

        {#if manageableCrews.length === 0}
          <p class="text-sm text-gray-500 bg-gray-50 p-4 rounded-lg">관리할 수 있는 크루가 없습니다.</p>
        {:else}
          <select
            class="w-full px-3 py-2 border border-gray-200 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
            value={selectedCrewId}
            onchange={(e) => selectCrew(e.currentTarget.value)}
          >
            <option value="">크루 선택...</option>
            {#each manageableCrews as c}
              <option value={c.id}>{c.name} ({c.visibility === 'Private' ? '비공개' : '공개'})</option>
            {/each}
          </select>

          {#if manageError}
            <p class="text-sm text-red-600 bg-red-50 px-3 py-2 rounded-lg">{manageError}</p>
          {/if}

          {#if selectedCrewId}
            <div class="flex gap-2 items-center">
              <input
                type="text"
                bind:value={inviteUsername}
                placeholder="초대할 아이디"
                class="flex-1 px-3 py-2 border border-gray-200 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
              />
              <select bind:value={inviteRole} class="px-2 py-2 border border-gray-200 rounded-lg text-sm">
                <option value={2}>Member</option>
                <option value={1}>Manager</option>
              </select>
              <button
                type="button"
                onclick={inviteMember}
                class="px-4 py-2 bg-indigo-600 text-white text-sm font-semibold rounded-lg hover:bg-indigo-700"
              >
                초대
              </button>
            </div>

            {#if manageableCrews.find((c) => c.id === selectedCrewId)?.my_role === 0}
              <p class="text-xs text-gray-400">Crew 삭제는 아래 «Crew 삭제» 섹션을 이용하세요.</p>
            {/if}

            {#if crewMembers.length > 0}
              <ul class="space-y-2">
                {#each crewMembers as m}
                  <li class="flex items-center justify-between border border-gray-200 rounded-lg px-3 py-2">
                    <div>
                      <p class="text-sm font-medium text-gray-800">{m.username}</p>
                      <p class="text-[10px] text-gray-400">
                        {ROLE_LABELS[m.role] ?? m.role} · {STATUS_LABELS[m.status] ?? m.status}
                      </p>
                    </div>
                    <div class="flex gap-2">
                      {#if m.status === 0 || m.status === 2}
                        <button
                          type="button"
                          onclick={() => approveMember(m.user_id)}
                          class="text-sm text-indigo-600 hover:text-indigo-800 font-medium"
                        >
                          {m.status === 0 ? '승인' : '초대확정'}
                        </button>
                      {/if}
                      {#if m.status === 1 && m.role !== 0}
                        <button
                          type="button"
                          onclick={() => banMember(m.user_id)}
                          class="text-sm text-red-600 hover:text-red-700 font-medium"
                        >
                          차단
                        </button>
                      {/if}
                    </div>
                  </li>
                {/each}
              </ul>
            {:else}
              <p class="text-sm text-gray-500">멤버가 없습니다.</p>
            {/if}
          {/if}
        {/if}
      </section>

      {#if deletableCrews.length > 0}
        <section class="space-y-3 mt-8 pt-6 border-t border-gray-100">
          <h3 class="text-sm font-bold text-gray-700 uppercase tracking-wide">Crew 삭제</h3>
          <p class="text-xs text-gray-500 leading-relaxed">
            <strong>직접 Owner</strong>이거나 <strong>상위 Crew Owner</strong>인 경우, 가입하지 않은 하위 Crew도 삭제할 수 있습니다.
            하위 Crew·파일·폴더가 연쇄 삭제됩니다.
          </p>
          <select
            class="w-full px-3 py-2 border border-gray-200 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-red-400"
            bind:value={deleteTargetCrewId}
          >
            <option value="">삭제할 Crew 선택...</option>
            {#each deletableCrews as c}
              <option value={c.id}>
                {c.name} ({c.visibility === 'Private' ? '비공개' : '공개'}{c.is_direct_owner ? '' : ' · 상위 Owner'})
              </option>
            {/each}
          </select>
          <button
            type="button"
            onclick={deleteSelectedCrew}
            disabled={!deleteTargetCrewId}
            class="w-full px-4 py-2 border border-red-200 text-red-600 text-sm font-semibold rounded-lg hover:bg-red-50 disabled:opacity-40 disabled:cursor-not-allowed"
          >
            선택한 Crew 영구 삭제
          </button>
        </section>
      {/if}

      {#if canManageGlobal}
        <section class="space-y-3 mt-8 pt-6 border-t border-gray-100">
          <h3 class="text-sm font-bold text-gray-700 uppercase tracking-wide">전역(글로벌) 크루 설정</h3>
          <p class="text-xs text-gray-500 leading-relaxed">
            신규 회원가입은 <strong>가입 승인제</strong>입니다. 가입한 사용자는 글로벌 크루 멤버로 <strong>가입대기(Pending)</strong> 상태가 되며, 아래에서 승인할 수 있습니다.
          </p>
          {#if globalPendingMembers.length > 0}
            <ul class="space-y-2">
              {#each globalPendingMembers as m}
                <li class="flex items-center justify-between border border-amber-200 bg-amber-50 rounded-lg px-3 py-2">
                  <div>
                    <p class="text-sm font-medium text-gray-800">{m.username}</p>
                    <p class="text-[10px] text-gray-500">가입 승인 대기</p>
                  </div>
                  <button
                    type="button"
                    onclick={() => approveGlobalMember(m.user_id)}
                    class="text-sm text-indigo-600 hover:text-indigo-800 font-medium"
                  >
                    승인
                  </button>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="text-sm text-gray-500 bg-gray-50 p-3 rounded-lg">승인 대기 중인 가입 신청이 없습니다.</p>
          {/if}
          {#if globalActiveMembers.length > 0}
            <div class="pt-2">
              <p class="text-xs font-semibold text-gray-600 mb-2">활성 멤버 관리</p>
              <ul class="space-y-2">
                {#each globalActiveMembers as m}
                  <li class="flex items-center justify-between border border-gray-200 rounded-lg px-3 py-2">
                    <div>
                      <p class="text-sm font-medium text-gray-800">{m.username}</p>
                      <p class="text-[10px] text-gray-400">{ROLE_LABELS[m.role] ?? m.role} · 활성</p>
                    </div>
                    <button
                      type="button"
                      onclick={() => banGlobalMember(m.user_id)}
                      class="text-sm text-red-600 hover:text-red-700 font-medium"
                    >
                      차단
                    </button>
                  </li>
                {/each}
              </ul>
            </div>
          {/if}
          <p class="text-xs text-gray-500 leading-relaxed pt-2">
            전역 홈의 하위 크루 최대 depth(<strong>max_sub_crew_depth</strong>)입니다.
            값을 늘리거나 줄이면 그 차이만큼 <strong>모든 하위 크루의 depth 제약도 함께</strong> 조정됩니다.
          </p>
          <div class="flex gap-2 items-center">
            <input
              type="number"
              min="0"
              bind:value={globalDepthInput}
              class="w-28 px-3 py-2 border border-gray-200 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
            />
            <span class="text-xs text-gray-400">현재: {globalDepth}</span>
            <button
              type="button"
              onclick={saveGlobalDepth}
              class="px-4 py-2 bg-indigo-600 text-white text-sm font-semibold rounded-lg hover:bg-indigo-700"
            >
              저장
            </button>
          </div>
          {#if globalSettingsError}
            <p class="text-sm text-red-600 bg-red-50 px-3 py-2 rounded-lg">{globalSettingsError}</p>
          {/if}
        </section>

        <section class="space-y-3 mt-6 pt-6 border-t border-gray-100">
          <div class="flex items-center justify-between gap-2">
            <h3 class="text-sm font-bold text-gray-700 uppercase tracking-wide">감사 로그</h3>
            <button
              type="button"
              onclick={loadAuditLogs}
              class="text-xs text-indigo-600 hover:text-indigo-800 font-medium"
            >
              새로고침
            </button>
          </div>
          <p class="text-xs text-gray-500">로그인, 삭제, 휴지통, 멤버 차단 등 주요 작업이 기록됩니다.</p>
          {#if auditLogsLoading}
            <p class="text-sm text-gray-500">불러오는 중…</p>
          {:else if auditLogsError}
            <p class="text-sm text-red-600 bg-red-50 px-3 py-2 rounded-lg">{auditLogsError}</p>
          {:else if auditLogs.length === 0}
            <p class="text-sm text-gray-500">기록이 없습니다.</p>
          {:else}
            <div class="max-h-64 overflow-y-auto border border-gray-100 rounded-lg">
              <table class="w-full text-xs text-left">
                <thead class="bg-gray-50 text-gray-500 sticky top-0">
                  <tr>
                    <th class="px-2 py-1.5 font-medium">시각</th>
                    <th class="px-2 py-1.5 font-medium">사용자</th>
                    <th class="px-2 py-1.5 font-medium">작업</th>
                    <th class="px-2 py-1.5 font-medium">대상</th>
                    <th class="px-2 py-1.5 font-medium">IP</th>
                  </tr>
                </thead>
                <tbody class="divide-y divide-gray-50">
                  {#each auditLogs as log}
                    <tr class="hover:bg-gray-50/80">
                      <td class="px-2 py-1.5 whitespace-nowrap text-gray-500">{log.created_at}</td>
                      <td class="px-2 py-1.5">{log.username ?? log.user_id ?? '—'}</td>
                      <td class="px-2 py-1.5 font-medium text-gray-800">{log.action}</td>
                      <td class="px-2 py-1.5 text-gray-600 max-w-[8rem] truncate" title={log.detail ?? ''}>
                        {#if log.target_type}{log.target_type}{/if}
                        {#if log.target_id}:{log.target_id}{/if}
                        {#if log.detail}<span class="text-gray-400"> ({log.detail})</span>{/if}
                      </td>
                      <td class="px-2 py-1.5 text-gray-400">{log.ip_address ?? '—'}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {/if}
        </section>
      {/if}
      {/if}
    {:else}
      <form onsubmit={(e) => { e.preventDefault(); handleLogin(); }} class="space-y-5">
        <div>
          <label for="username" class="block text-sm font-semibold text-gray-700 mb-1.5">아이디</label>
          <input
            type="text"
            id="username"
            bind:value={username}
            placeholder="아이디를 입력하세요"
            class="w-full px-3 py-2.5 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
          />
        </div>

        <div>
          <label for="password" class="block text-sm font-semibold text-gray-700 mb-1.5">비밀번호</label>
          <input
            type="password"
            id="password"
            bind:value={password}
            placeholder="비밀번호"
            class="w-full px-3 py-2.5 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
          />
        </div>

        {#if errorMessage}
          <p class="text-sm text-red-600 bg-red-50 px-3 py-2 rounded-lg">{errorMessage}</p>
        {/if}

        <button
          type="submit"
          class="w-full py-2.5 bg-indigo-600 text-white font-semibold rounded-lg hover:bg-indigo-700 transition-colors"
        >
          로그인
        </button>
      </form>

      <div class="mt-6 pt-6 border-t border-gray-100 text-center">
        <p class="text-sm text-gray-500 mb-3">아직 계정이 없으신가요?</p>
        <button
          type="button"
          onclick={goToRegister}
          class="w-full py-2.5 border border-indigo-200 text-indigo-600 font-semibold rounded-lg hover:bg-indigo-50 transition-colors"
        >
          회원가입
        </button>
        <p class="text-xs text-gray-400 mt-2">가입 후 관리자 승인이 필요합니다.</p>
      </div>
    {/if}
  </div>
</main>
