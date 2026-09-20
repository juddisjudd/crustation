import tailwindcss from '@tailwindcss/vite';
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

const backend = process.env.CRUSTATION_URL ?? 'http://localhost:8080';

export default defineConfig({
	cacheDir: process.env.VITE_CACHE_DIR ?? 'node_modules/.vite',
	plugins: [
		tailwindcss(),
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			adapter: adapter({
				pages: 'build',
				assets: 'build',
				fallback: 'index.html',
				strict: false
			})
		})
	],
	server: {
		proxy: {
			'/api': { target: backend, changeOrigin: true },
			'/ws': { target: backend.replace(/^http/, 'ws'), ws: true }
		}
	}
});
