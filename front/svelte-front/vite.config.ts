import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit()],
    server: {
        host: true,
        port: 5173,
        allowedHosts: true,
        proxy: {
            '/api': {
                target: 'http://localhost:3000', // Rust 서버 주소
                changeOrigin: true,
                // 만약 Rust 백엔드 라우터에 이미 /api가 포함되어 있다면 
                // 아래 rewrite는 필요 없습니다.
                // rewrite: (path) => path.replace(/^\/api/, '') 
            }
        }
}
});
