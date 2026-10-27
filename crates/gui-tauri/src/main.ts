import App from './App.svelte';
import QuickExtract from './QuickExtract.svelte';
import { mount, tick } from 'svelte';
import { getCurrentWindow } from '@tauri-apps/api/window';

const component = new URLSearchParams(window.location.search).get('mode') === 'extract-here'
  ? QuickExtract
  : App;
const app = mount(component, {
  target: document.getElementById('app')!,
});

// Native windows start hidden so the browser cannot flash before the compact
// extraction view (or restored main view) has mounted.
void tick().then(() => getCurrentWindow().show()).catch((error) => {
  console.error('Failed to show application window', error);
});

export default app;
