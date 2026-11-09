// One static page: the Tauri app has no server to render it, so it renders in the
// webview (adapter-static's fallback index.html).
export const ssr = false;
export const prerender = false;
