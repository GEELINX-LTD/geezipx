<script lang="ts">
  import { settingsStore } from '../stores/settingsStore.svelte';
  import { settingsGuard } from '../stores/settingsGuard.svelte';
  import { localeStore } from '../stores/localeStore.svelte';
  import type { GeeZipXSettings } from '../bridge';
  import { getFormats, getVersion, getFileAssociations, setFileAssociation, openAssociationSettings, type FormatInfo, type AssociationsResult } from '../bridge';
  import { open } from '@tauri-apps/plugin-dialog';

  type Tab = 'general' | 'compression' | 'appearance' | 'associations' | 'about';

  let activeTab = $state<Tab>('general');
  let formData = $state<GeeZipXSettings>(settingsStore.DEFAULTS);
  let original = $state<GeeZipXSettings>(settingsStore.DEFAULTS);
  let loaded = $state(false);
  let saved = $state(false);
  let formats = $state<FormatInfo[]>([]);
  let associations = $state<AssociationsResult | null>(null);
  let assocBusy = $state<Record<string, boolean>>({});
  let levelInput = $state('');
  let version = $state('');

  /** True when the current form differs from the last loaded/saved snapshot. */
  let dirty = $derived(JSON.stringify(formData) !== JSON.stringify(original));

  // Keep the shared guard in sync so TabBar can prompt before navigating away.
  $effect(() => {
    settingsGuard.dirty = dirty;
  });

  $effect(() => {
    Promise.all([
      settingsStore.loadAll(),
      getFormats(),
      getFileAssociations().catch(() => null),
    ]).then(([d, f, a]) => {
      formData = { ...d };
      levelInput = d.default_level != null ? String(d.default_level) : '';
      original = { ...d };
      formats = f;
      associations = a;
      loaded = true;
    });
  });

  $effect(() => {
    getVersion().then(v => version = v);
  });

  async function browseOutput() {
    const selected = await open({ directory: true, multiple: false });
    if (selected && typeof selected === 'string') {
      formData = { ...formData, default_output_dir: selected };
    }
  }

  function clearOutput() {
    formData = { ...formData, default_output_dir: null };
  }

  function onLevelInput(e: Event) {
    levelInput = (e.currentTarget as HTMLInputElement).value;
  }

  function onLevelBlur() {
    if (levelInput.trim() === '') {
      formData = { ...formData, default_level: null };
      return;
    }
    const n = parseInt(levelInput, 10);
    if (Number.isNaN(n)) {
      // Revert to the last valid value instead of clearing.
      levelInput = formData.default_level != null ? String(formData.default_level) : '';
      return;
    }
    const c = Math.max(0, Math.min(22, Math.round(n)));
    levelInput = String(c);
    formData = { ...formData, default_level: c };
  }

  async function handleSave() {
    // --- Validation / normalization ---
    const next = { ...formData };

    // Clamp compression level to the engine's accepted range.
    if (next.default_level !== null) {
      let lvl = Math.round(next.default_level);
      if (lvl < 0) lvl = 0;
      if (lvl > 22) lvl = 22;
      next.default_level = lvl;
    }

    // Fall back to a valid format if the stored one was removed.
    if (!formats.some((f) => f.can_compress && f.name === next.default_format)) {
      next.default_format = 'zip';
    }

    // Do not persist the password unless the user opted in.
    if (!next.remember_password) {
      next.default_password = null;
    }

    formData = next;
    await settingsStore.saveAll(formData);
    original = { ...formData };
    settingsGuard.dirty = false;

    // Apply theme immediately
    const d = document.documentElement;
    if (formData.theme === 'light') {
      d.setAttribute('data-theme', 'light');
    } else if (formData.theme === 'dark') {
      d.setAttribute('data-theme', 'dark');
    } else {
      d.removeAttribute('data-theme');
    }

    // Apply locale if changed
    if (formData.locale !== localeStore.locale) {
      localeStore.switchLocale(formData.locale);
    }

    saved = true;
    setTimeout(() => (saved = false), 2000);
  }

  function handleCancel() {
    formData = { ...original };
    settingsGuard.dirty = false;
  }

  function handleReset() {
    formData = { ...settingsStore.DEFAULTS };
  }

  async function toggleAssociation(item: AssocItem, enabled: boolean) {
    assocBusy = { ...assocBusy, [item.ext]: true };
    try {
      await setFileAssociation(item.ext, enabled);
      associations = await getFileAssociations();
    } catch (e) {
      console.error('setFileAssociation failed', e);
    } finally {
      assocBusy = { ...assocBusy, [item.ext]: false };
    }
  }

  async function openAssocSettings(ext?: string) {
    try {
      await openAssociationSettings(ext);
    } catch (e) {
      console.error('openAssociationSettings failed', e);
    }
  }
</script>

<div class="page">
  <h1>{localeStore.t('settings.title')}</h1>

  {#if !loaded}
    <p class="loading">{localeStore.t('common.loading')}</p>
  {:else}
    <!-- Tabs -->
    <nav class="tab-bar">
      <button class="tab-btn" class:active={activeTab === 'general'} onclick={() => (activeTab = 'general')}>
        {localeStore.t('settings.general')}
      </button>
      <button class="tab-btn" class:active={activeTab === 'compression'} onclick={() => (activeTab = 'compression')}>
        {localeStore.t('settings.compression')}
      </button>
      <button class="tab-btn" class:active={activeTab === 'appearance'} onclick={() => (activeTab = 'appearance')}>
        {localeStore.t('settings.appearance')}
      </button>
      <button class="tab-btn" class:active={activeTab === 'associations'} onclick={() => (activeTab = 'associations')}>
        {localeStore.t('settings.fileAssociations.label')}
      </button>
      <button class="tab-btn" class:active={activeTab === 'about'} onclick={() => (activeTab = 'about')}>
        {localeStore.t('settings.about.label')}
      </button>
    </nav>

    <!-- Tab content -->
    <div class="tab-content">
      {#if activeTab === 'general'}
        <!-- Language -->
        <div class="field">
          <label class="field-label" for="setting-locale">{localeStore.t('settings.locale.label')}</label>
          <p class="field-help">{localeStore.t('settings.locale.help')}</p>
          <select id="setting-locale" value={formData.locale} onchange={(e) => (formData = { ...formData, locale: e.currentTarget.value as 'en' | 'zh-CN' })}>
            <option value="zh-CN">简体中文</option>
            <option value="en">English</option>
          </select>
        </div>

        <!-- Default output directory -->
        <div class="field">
          <span class="field-label">{localeStore.t('settings.defaultOutputDir.label')}</span>
          <p class="field-help">{localeStore.t('settings.defaultOutputDir.help')}</p>
          <div class="input-row">
            <input
              type="text"
              placeholder="/home/user/archives"
              value={formData.default_output_dir ?? ''}
              oninput={(e) => (formData = { ...formData, default_output_dir: (e.currentTarget as HTMLInputElement).value || null })}
            />
            <button class="btn-secondary" onclick={browseOutput}>
              {localeStore.t('settings.defaultOutputDir.browse')}
            </button>
            {#if formData.default_output_dir}
              <button class="btn-tertiary" onclick={clearOutput}>✕</button>
            {/if}
          </div>
        </div>

        <!-- Overwrite strategy -->
        <fieldset class="radio-fieldset">
          <legend class="field-label">{localeStore.t('settings.overwriteStrategy.label')}</legend>
          <div class="radio-group">
            <label class="radio-item">
              <input
                type="radio"
                name="ow-strategy"
                value="prompt"
                checked={formData.overwrite_strategy === 'prompt'}
                onchange={() => (formData = { ...formData, overwrite_strategy: 'prompt' })}
              />
              <span>{localeStore.t('settings.overwriteStrategy.prompt')}</span>
            </label>
            <label class="radio-item">
              <input
                type="radio"
                name="ow-strategy"
                value="skip"
                checked={formData.overwrite_strategy === 'skip'}
                onchange={() => (formData = { ...formData, overwrite_strategy: 'skip' })}
              />
              <span>{localeStore.t('settings.overwriteStrategy.skip')}</span>
            </label>
            <label class="radio-item">
              <input
                type="radio"
                name="ow-strategy"
                value="overwrite"
                checked={formData.overwrite_strategy === 'overwrite'}
                onchange={() => (formData = { ...formData, overwrite_strategy: 'overwrite' })}
              />
              <span>{localeStore.t('settings.overwriteStrategy.overwrite')}</span>
            </label>
          </div>
        </fieldset>

        <!-- On complete -->
        <fieldset class="radio-fieldset">
          <legend class="field-label">{localeStore.t('settings.onComplete.label')}</legend>
          <p class="field-help">{localeStore.t('settings.onComplete.help')}</p>
          <div class="radio-group">
            <label class="radio-item">
              <input
                type="radio"
                name="on-complete"
                value="nothing"
                checked={formData.on_complete === 'nothing'}
                onchange={() => (formData = { ...formData, on_complete: 'nothing' })}
              />
              <span>{localeStore.t('settings.onComplete.nothing')}</span>
            </label>
            <label class="radio-item">
              <input
                type="radio"
                name="on-complete"
                value="open_output"
                checked={formData.on_complete === 'open_output'}
                onchange={() => (formData = { ...formData, on_complete: 'open_output' })}
              />
              <span>{localeStore.t('settings.onComplete.openOutput')}</span>
            </label>
          </div>
        </fieldset>

      {:else if activeTab === 'compression'}
        <!-- Default format -->
        <div class="field">
          <label class="field-label" for="setting-format">{localeStore.t('settings.defaultFormat.label')}</label>
          <select
            id="setting-format"
            value={formData.default_format}
            onchange={(e) => (formData = { ...formData, default_format: e.currentTarget.value })}
          >
            {#each formats.filter(f => f.can_compress) as fmt (fmt.name)}
              <option value={fmt.name}>{fmt.name.toUpperCase()}</option>
            {/each}
          </select>
        </div>

        <!-- Default level -->
        <div class="field">
          <label class="field-label" for="setting-level">{localeStore.t('settings.defaultLevel.label')}</label>
          <p class="field-help">{localeStore.t('settings.defaultLevel.help')}</p>
          <input
            type="number"
            id="setting-level"
            min="0"
            max="22"
            value={levelInput}
            placeholder="Auto"
            oninput={onLevelInput}
            onblur={onLevelBlur}
          />
        </div>

        <!-- Recursive -->
        <div class="field">
          <label class="checkbox-row">
            <input
              type="checkbox"
              checked={formData.recursive}
              onchange={(e) => (formData = { ...formData, recursive: (e.currentTarget as HTMLInputElement).checked })}
            />
            <span>{localeStore.t('settings.recursive.label')}</span>
          </label>
        </div>

        <!-- Default password -->
        <div class="field">
          <label class="field-label" for="setting-password">{localeStore.t('settings.defaultPassword.label')}</label>
          <p class="field-help">{localeStore.t('settings.defaultPassword.help')}</p>
          <input
            type="password"
            id="setting-password"
            placeholder="••••••"
            value={formData.default_password ?? ''}
            oninput={(e) => (formData = { ...formData, default_password: (e.currentTarget as HTMLInputElement).value || null })}
          />
        </div>

        <!-- Remember password -->
        <div class="field">
          <label class="checkbox-row">
            <input
              type="checkbox"
              checked={formData.remember_password}
              onchange={(e) => (formData = { ...formData, remember_password: (e.currentTarget as HTMLInputElement).checked })}
            />
            <span>{localeStore.t('settings.rememberPassword.label')}</span>
          </label>
        </div>

      {:else if activeTab === 'appearance'}
        <!-- Theme -->
        <fieldset class="radio-fieldset">
          <legend class="field-label">{localeStore.t('settings.theme.label')}</legend>
          <div class="radio-group">
            <label class="radio-item">
              <input
                type="radio"
                name="theme"
                value="system"
                checked={formData.theme === 'system'}
                onchange={() => (formData = { ...formData, theme: 'system' })}
              />
              <span>{localeStore.t('settings.theme.system')}</span>
            </label>
            <label class="radio-item">
              <input
                type="radio"
                name="theme"
                value="light"
                checked={formData.theme === 'light'}
                onchange={() => (formData = { ...formData, theme: 'light' })}
              />
              <span>{localeStore.t('settings.theme.light')}</span>
            </label>
            <label class="radio-item">
              <input
                type="radio"
                name="theme"
                value="dark"
                checked={formData.theme === 'dark'}
                onchange={() => (formData = { ...formData, theme: 'dark' })}
              />
              <span>{localeStore.t('settings.theme.dark')}</span>
            </label>
          </div>
        </fieldset>
      {:else if activeTab === 'associations'}
        {#if !associations}
          <p class="loading">{localeStore.t('common.loading')}</p>
        {:else}
          <p class="field-help">{localeStore.t('settings.fileAssociations.help')}</p>

          {#if !associations.can_set_default}
            <div class="notice">
              <p>{localeStore.t('settings.fileAssociations.windowsNote')}</p>
              <button class="btn-secondary" onclick={() => openAssocSettings()}>
                {localeStore.t('settings.fileAssociations.openSettings')}
              </button>
            </div>
          {/if}

          <div class="assoc-list">
            {#each associations.items as item (item.ext)}
              {@const bound = item.is_default ?? item.is_registered}
              <div class="assoc-row">
                <div class="assoc-info">
                  <div class="assoc-name">
                    {item.name}
                    {#each item.exts as e (e)}
                      <span class="chip">{e}</span>
                    {/each}
                  </div>
                  <div class="assoc-desc">{item.description}</div>
                  <div class="assoc-status">
                    {#if item.is_default === true}
                      <span class="badge badge-default">{localeStore.t('settings.fileAssociations.default')}</span>
                    {:else if item.is_registered}
                      <span class="badge badge-reg">{localeStore.t('settings.fileAssociations.registered')}</span>
                    {:else}
                      <span class="badge badge-none">{localeStore.t('settings.fileAssociations.notBound')}</span>
                    {/if}
                  </div>
                </div>
                <div class="assoc-actions">
                  <label class="checkbox-row">
                    <input
                      type="checkbox"
                      checked={bound}
                      disabled={assocBusy[item.ext]}
                      onchange={(e) => toggleAssociation(item, (e.currentTarget as HTMLInputElement).checked)}
                    />
                    <span>{localeStore.t('settings.fileAssociations.bind')}</span>
                  </label>
                  {#if !associations.can_set_default}
                    <button class="btn-tertiary" onclick={() => openAssocSettings(item.ext)}>
                      {localeStore.t('settings.fileAssociations.setDefault')}
                    </button>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        {/if}
      {:else if activeTab === 'about'}
        <div class="about-card">
          <h2 class="about-title">GeeZipX</h2>
          <p class="about-version">
            {localeStore.t('settings.about.version')}: {version || localeStore.t('common.loading')}
          </p>
          <p class="about-desc">{localeStore.t('settings.about.description')}</p>
          <p class="about-stack">{localeStore.t('settings.about.stack')}</p>
          <a
            class="about-link"
            href="https://github.com/GEELINX-LTD/geezipx"
            target="_blank"
            rel="noopener"
          >
            {localeStore.t('settings.about.repo')}
          </a>
        </div>
      {/if}
    </div>

    <!-- Action buttons -->
    <div class="action-bar">
      {#if dirty}
        <span class="dirty-msg">{localeStore.t('settings.dirty')}</span>
      {/if}
      {#if saved}
        <span class="saved-msg">{localeStore.t('settings.saved')}</span>
      {/if}
      <button class="btn-tertiary" onclick={handleReset} disabled={!dirty}>{localeStore.t('settings.reset')}</button>
      <button class="btn-secondary" onclick={handleCancel} disabled={!dirty}>{localeStore.t('settings.cancel')}</button>
      <button class="btn-primary" onclick={handleSave} disabled={!dirty}>{localeStore.t('settings.save')}</button>
    </div>
  {/if}
</div>

<style>
  .page {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    padding: var(--space-5);
  }

  h1 {
    font-size: var(--text-2xl);
    font-weight: 600;
    margin-bottom: var(--space-5);
  }

  .loading {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  /* --- Tab bar --- */
  .tab-bar {
    display: flex;
    gap: 0;
    border-bottom: 2px solid var(--color-border-light);
    margin-bottom: var(--space-5);
  }

  .tab-btn {
    padding: var(--space-2) var(--space-4);
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--color-text-secondary);
    border-bottom: 2px solid transparent;
    margin-bottom: -2px;
    transition: color var(--transition-fast), border-color var(--transition-fast);
  }

  .tab-btn:hover {
    color: var(--color-text);
  }

  .tab-btn.active {
    color: var(--color-accent);
    border-bottom-color: var(--color-accent);
  }

  /* --- Tab content --- */
  .tab-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
  }

  /* --- Form fields --- */
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .field-label {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--color-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .field-help {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
    margin-bottom: var(--space-1);
  }

  .input-row {
    display: flex;
    gap: var(--space-2);
  }

  .input-row input {
    flex: 1;
  }

  select, input {
    padding: var(--space-2) var(--space-3);
  }

  input[type="number"] {
    max-width: 120px;
  }

  /* --- Radio fieldset --- */
  .radio-fieldset {
    border: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .radio-fieldset > legend {
    padding: 0;
    margin-bottom: var(--space-1);
  }

  /* --- Radio group --- */
  .radio-group {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding-top: var(--space-1);
  }

  .radio-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .radio-item input[type="radio"] {
    accent-color: var(--color-accent);
    width: 16px;
    height: 16px;
  }

  /* --- Checkbox --- */
  .checkbox-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
    cursor: pointer;
    padding-top: var(--space-1);
  }

  .checkbox-row input[type="checkbox"] {
    accent-color: var(--color-accent);
    width: 16px;
    height: 16px;
  }

  /* --- Action bar --- */
  .action-bar {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-3);
    align-items: center;
    padding-top: var(--space-4);
    margin-top: var(--space-4);
    border-top: 1px solid var(--color-border-light);
  }

  .dirty-msg {
    font-size: var(--text-sm);
    color: var(--color-warning, #b7791f);
    margin-right: auto;
  }

  .saved-msg {
    font-size: var(--text-sm);
    color: var(--color-success);
    margin-right: auto;
  }

  .btn-primary {
    padding: var(--space-2) var(--space-5);
    background: var(--color-accent);
    color: #fff;
    border-radius: var(--radius-md);
    font-weight: 500;
    font-size: var(--text-sm);
  }

  .btn-primary:hover {
    background: var(--color-accent-hover);
  }

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-secondary {
    padding: var(--space-2) var(--space-3);
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    font-size: var(--text-sm);
  }

  .btn-secondary:hover {
    background: var(--color-surface-alt);
  }

  .btn-secondary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-tertiary {
    padding: var(--space-2) var(--space-3);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    border-radius: var(--radius-sm);
  }

  .btn-tertiary:hover {
    color: var(--color-text);
    background: var(--color-surface-alt);
  }

  .btn-tertiary:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  /* --- File associations tab --- */
  .notice {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-alt);
    margin-bottom: var(--space-4);
  }

  .notice p {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
    margin: 0;
  }

  .assoc-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .assoc-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-3);
    border: 1px solid var(--color-border-light);
    border-radius: var(--radius-md);
  }

  .assoc-info {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .assoc-name {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--color-text);
  }

  .chip {
    font-size: var(--text-xs);
    font-weight: 500;
    color: var(--color-text-secondary);
    background: var(--color-surface-alt);
    border: 1px solid var(--color-border-light);
    border-radius: var(--radius-sm);
    padding: 1px var(--space-2);
  }

  .assoc-desc {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
  }

  .assoc-status {
    display: flex;
    gap: var(--space-2);
  }

  .badge {
    font-size: var(--text-xs);
    padding: 1px var(--space-2);
    border-radius: var(--radius-sm);
    font-weight: 500;
  }

  .badge-default {
    color: var(--color-success, #2f855a);
    background: color-mix(in srgb, var(--color-success, #2f855a) 15%, transparent);
  }

  .badge-reg {
    color: var(--color-warning, #b7791f);
    background: color-mix(in srgb, var(--color-warning, #b7791f) 15%, transparent);
  }

  .badge-none {
    color: var(--color-text-muted);
    background: var(--color-surface-alt);
  }

  .assoc-actions {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  /* --- About tab --- */
  .about-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-4);
    padding: var(--space-8) var(--space-5);
    text-align: center;
  }

  .about-title {
    font-size: var(--text-3xl);
    font-weight: 700;
    color: var(--color-accent);
    letter-spacing: -0.02em;
  }

  .about-version {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    font-family: var(--font-mono, 'SF Mono', 'Fira Code', monospace);
  }

  .about-desc {
    max-width: 480px;
    font-size: var(--text-sm);
    color: var(--color-text-muted);
    line-height: 1.6;
  }

  .about-stack {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
    padding: var(--space-1) var(--space-3);
    background: var(--color-surface-alt);
    border-radius: var(--radius-sm);
  }

  .about-link {
    font-size: var(--text-sm);
    color: var(--color-accent);
    text-decoration: none;
    font-weight: 500;
    padding: var(--space-2) var(--space-4);
    border: 1px solid var(--color-accent);
    border-radius: var(--radius-md);
    transition: background var(--transition-fast), color var(--transition-fast);
  }

  .about-link:hover {
    background: var(--color-accent);
    color: #fff;
  }
</style>
