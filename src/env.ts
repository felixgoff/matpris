import { defineEnvVars } from '@sveltejs/kit/env';

export const variables = defineEnvVars({
	PUBLIC_MAPKIT_TOKEN: {
		public: true,
		// Built into the page: the Tauri app has no server to read it from when it runs.
		static: true,
		description: 'MapKit JS token (JWT, scope "mapkit_js"). Built into the page, so a new token needs a rebuild.',
		// Optional: without it the app still works, the map just shows a message.
		schema: (value) => value || undefined
	}
});
