<script>
  import { goto } from '$app/navigation';
  import { base } from '$app/paths';
  import { getNasUrl } from '$lib/constants';

  let username = '';
  let password = '';
  let confirmPassword = '';
  let errorMessage = '';
  let successMessage = '';

  async function handleRegister() {
    errorMessage = '';
    successMessage = '';

    if (!username || !password) {
      errorMessage = '아이디와 비밀번호를 모두 입력해주세요.';
      return;
    }
    if (password.length < 8) {
      errorMessage = '비밀번호는 8자 이상이어야 합니다.';
      return;
    }
    if (password !== confirmPassword) {
      errorMessage = '비밀번호가 일치하지 않습니다.';
      return;
    }

    try {
      const response = await fetch(`${getNasUrl()}/api/users/register`, {
        method: 'POST',
        credentials: 'include',
        headers: {
          'Content-Type': 'application/json'
        },
        body: JSON.stringify({ username, password })
      });

      if (response.ok) {
        const data = await response.json();
        successMessage = data.message || '가입 신청이 접수되었습니다.';
        username = '';
        password = '';
        confirmPassword = '';
      } else {
        let msg = '회원가입에 실패했습니다. 아이디가 중복되었을 수 있습니다.';
        try {
          const data = await response.json();
          if (data?.error) msg = data.error;
          else if (data?.message) msg = data.message;
        } catch (_) {}
        errorMessage = msg;
      }
    } catch (error) {
      console.error('API Error:', error);
      errorMessage = '서버와 통신할 수 없습니다. 백엔드가 켜져 있는지 확인해 주세요.';
    }
  }

  function goToLogin() {
    goto(`${base}/login`);
  }

  function goToHome() {
    goto(`${base}/`);
  }
</script>

<main class="min-h-screen bg-gray-50 flex items-center justify-center p-4">
  <div class="w-full max-w-md bg-white rounded-2xl shadow-lg border border-gray-100 p-8">
    <button
      type="button"
      onclick={goToHome}
      class="text-sm text-gray-500 hover:text-indigo-600 mb-6"
    >
      ← NAS 홈으로
    </button>

    <h2 class="text-2xl font-bold text-gray-800 mb-2">회원가입</h2>
    <p class="text-sm text-gray-500 mb-8">
      계정을 만든 뒤 글로벌 크루 관리자의 승인을 받아야 NAS를 이용할 수 있습니다.
    </p>

    <form onsubmit={(e) => { e.preventDefault(); handleRegister(); }} class="space-y-5">
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
        <label for="password" class="block text-sm font-semibold text-gray-700 mb-1.5">비밀번호 <span class="text-xs font-normal text-gray-400">(8자 이상)</span></label>
        <input
          type="password"
          id="password"
          bind:value={password}
          placeholder="비밀번호 (8자 이상)"
          class="w-full px-3 py-2.5 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
        />
      </div>

      <div>
        <label for="confirmPassword" class="block text-sm font-semibold text-gray-700 mb-1.5">비밀번호 확인</label>
        <input
          type="password"
          id="confirmPassword"
          bind:value={confirmPassword}
          placeholder="비밀번호 확인"
          class="w-full px-3 py-2.5 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
        />
      </div>

      {#if errorMessage}
        <p class="text-sm text-red-600 bg-red-50 px-3 py-2 rounded-lg">{errorMessage}</p>
      {/if}
      {#if successMessage}
        <p class="text-sm text-green-600 bg-green-50 px-3 py-2 rounded-lg">{successMessage}</p>
      {/if}

      <button
        type="submit"
        class="w-full py-2.5 bg-indigo-600 text-white font-semibold rounded-lg hover:bg-indigo-700 transition-colors"
      >
        가입 신청
      </button>
    </form>

    <div class="mt-6 pt-6 border-t border-gray-100 text-center">
      <p class="text-sm text-gray-500 mb-3">이미 계정이 있으신가요?</p>
      <button
        type="button"
        onclick={goToLogin}
        class="w-full py-2.5 border border-indigo-200 text-indigo-600 font-semibold rounded-lg hover:bg-indigo-50 transition-colors"
      >
        로그인
      </button>
    </div>
  </div>
</main>
