import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		host: true,
		port: 5173,
		allowedHosts: true,
		proxy: {
			'/NAS/api': {
				target: 'http://localhost:3000',
				changeOrigin: true
			},
			'/webdav': {
				target: 'http://localhost:3000',
				changeOrigin: true
			}
		}
	}
});
