import { getNasUrl } from '$lib/constants';

const STORAGE_KEY = 'nas_auth';

function readStored() {
  if (typeof localStorage === 'undefined') return null;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    if (parsed?.userId && parsed?.username) return parsed;
    return null;
  } catch {
    return null;
  }
}

// 인증의 진짜 상태는 HttpOnly 세션 쿠키에 있다.
// 아래 localStorage 값은 UI 표시용 캐시일 뿐이며, refresh()로 서버와 동기화한다.
export const authState = $state({
  userId: readStored()?.userId ?? null,
  username: readStored()?.username ?? null,

  get isLoggedIn() {
    return this.userId !== null;
  },

  setSession(userId, username) {
    this.userId = userId;
    this.username = username;
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(STORAGE_KEY, JSON.stringify({ userId, username }));
    }
  },

  clear() {
    this.userId = null;
    this.username = null;
    if (typeof localStorage !== 'undefined') {
      localStorage.removeItem(STORAGE_KEY);
    }
  },

  /** 세션 쿠키 유효성을 서버에 확인한다. 무효면 로컬 상태를 비운다. */
  async refresh() {
    try {
      const res = await fetch(`${getNasUrl()}/api/users/me`, {
        credentials: 'include'
      });
      if (res.ok) {
        const data = await res.json();
        this.userId = data.user_id;
        if (typeof localStorage !== 'undefined') {
          localStorage.setItem(
            STORAGE_KEY,
            JSON.stringify({ userId: this.userId, username: this.username })
          );
        }
        return true;
      }
      this.clear();
      return false;
    } catch {
      return false;
    }
  },

  /** 서버 세션을 폐기하고 로컬 상태를 비운다. */
  async logout() {
    try {
      await fetch(`${getNasUrl()}/api/users/logout`, {
        method: 'POST',
        credentials: 'include'
      });
    } catch {
      // 네트워크 실패해도 로컬은 정리한다.
    }
    this.clear();
  }
});
