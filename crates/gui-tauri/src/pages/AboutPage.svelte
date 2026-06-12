<script lang="ts">
  import { localeStore } from '../stores/localeStore.svelte';
</script>

<div class="page">
  <h1>{localeStore.t('about.title')}</h1>
  <p class="version">v0.5.0</p>
  <p>{localeStore.t('about.description')}</p>
  <p class="stack">{localeStore.t('about.stack')}</p>
</div>
<style>
  .page { padding: var(--space-6); }
  h1 { font-size: var(--text-2xl); font-weight: 600; margin-bottom: var(--space-2); }
  .version { color: var(--color-text-secondary); margin-bottom: var(--space-4); }
  p { margin-bottom: var(--space-2); line-height: 1.6; }
  .stack { color: var(--color-text-muted); font-size: var(--text-sm); }
</style>
