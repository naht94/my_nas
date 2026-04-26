import adapter from '@sveltejs/adapter-static'; // auto 대신 static으로 변경

/** @type {import('@sveltejs/kit').Config} */
const config = {
	kit: {
		adapter: adapter({
			// 빌드 결과물이 저장될 폴더 이름 (Rust에서 여기를 참조하게 됨)
			pages: 'build',
			assets: 'build',
			fallback: 'index.html', // SPA 모드를 위해 필수!
			precompress: false,
			strict: true
		}),
        paths: {
            base: '/NAS'
        }
	}
};

export default config;
