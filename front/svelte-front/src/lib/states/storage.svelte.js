import {NAS_URL} from '$lib/constants'
let storage = $state({ available: 0, total: 0, used: 0 });

export const storageState = {
    get current() { return storage; },
    // 💡 필요할 때만 호출할 수 있는 함수
    async refresh() {
        const res = await fetch(`${NAS_URL}/api/storage/usage`);
        storage = await res.json();
    }
};
