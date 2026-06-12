<script lang="ts">
  import { localeStore } from '../stores/localeStore.svelte';
</script>
<div class="page">
  <h1>{localeStore.t('settings.title')}</h1>
  <div class="field">
    <label for="locale-select">{localeStore.t('settings.locale.label')}</label>
    <select id="locale-select" value={localeStore.locale} onchange={(e) => localeStore.switchLocale(e.currentTarget.value as 'en' | 'zh-CN')}>
      <option value="zh-CN">简体中文</option>
      <option value="en">English</option>
    </select>
  </div>
</div>
<style>
  .page { padding: var(--space-6); }
  h1 { font-size: var(--text-2xl); font-weight: 600; margin-bottom: var(--space-5); }
  .field { margin-bottom: var(--space-4); }
  label { display: block; font-size: var(--text-sm); font-weight: 500; margin-bottom: var(--space-1); color: var(--color-text-secondary); }
  select { min-width: 200px; padding: var(--space-2); }
</style>
