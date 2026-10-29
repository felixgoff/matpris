import { defineEnvVars } from '@sveltejs/kit/env';

export const variables = defineEnvVars({
	PUBLIC_MAPKIT_TOKEN: {
		public: true,
		description: 'MapKit JS token (JWT, scope "mapkit_js"). Read at startup, so a new token only needs a restart.',
		// Optional: without it the app still works, the map just shows a message.
		schema: (value) => value || undefined
	}
});
