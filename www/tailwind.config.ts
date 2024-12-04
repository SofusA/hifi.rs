import type { Config } from 'tailwindcss';
import safeArea from 'tailwindcss-safe-area';

export default {
	content: ['./src/**/*.{html,js,svelte,ts}'],
	plugins: [safeArea],
	future: {
		hoverOnlyWhenSupported: true
	}
} satisfies Config;
