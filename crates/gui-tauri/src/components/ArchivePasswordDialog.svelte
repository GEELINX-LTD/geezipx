<script lang="ts">
  import { tick } from 'svelte';
  import { appStore } from '../stores/appStore.svelte';
  import { archiveManager } from '../stores/archiveStore.svelte';
  import { localeStore } from '../stores/localeStore.svelte';

  interface Props {
    tabId: string;
  }

  let { tabId }: Props = $props();
  let password = $state('');
  let passwordInput: HTMLInputElement | undefined;
  let challenge = $derived(archiveManager.tabs.get(tabId)?.passwordChallenge ?? null);
  let archiveName = $derived(
    challenge?.path.split(/[/\\]/).pop() || challenge?.path || '',
  );

  function trackPasswordInput(node: HTMLInputElement): () => void {
    passwordInput = node;
    if (node.closest('dialog')?.open) node.focus();
    return () => {
      if (passwordInput === node) passwordInput = undefined;
    };
  }

  function openModal(node: HTMLDialogElement): () => void {
    const activeElement = document.activeElement;
    const returnFocus = activeElement instanceof HTMLElement && activeElement !== document.body
      ? activeElement
      : null;

    if (!node.open) node.showModal();
    passwordInput?.focus();

    return () => {
      if (node.open) node.close();

      const currentTab = document.querySelector<HTMLElement>(
        `[data-tab-id="${CSS.escape(appStore.activeTab)}"]`,
      );
      const focusTarget = returnFocus?.isConnected ? returnFocus : currentTab;
      focusTarget?.focus();
    };
  }

  async function submitPassword(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (!password || challenge?.isSubmitting) return;

    const opened = await archiveManager.submitPassword(tabId, password);
    if (!opened && archiveManager.tabs.get(tabId)?.passwordChallenge) {
      password = '';
      await tick();
      passwordInput?.focus();
    }
  }

  function cancel(): void {
    password = '';
    const result = archiveManager.cancelPasswordChallenge(tabId);
    if (!result.closed) return;

    appStore.removeArchiveTab(tabId);
    appStore.switchTab(result.nextTabId ?? 'extract');
  }

  function handleCancel(event: Event): void {
    event.preventDefault();
    cancel();
  }
</script>

<dialog
  class="dialog"
  aria-labelledby="archive-password-title"
  aria-describedby="archive-password-description"
  oncancel={handleCancel}
  {@attach openModal}
>
  <form
    class="dialog-content"
    onsubmit={submitPassword}
  >
    <h2 id="archive-password-title">{localeStore.t('archivePassword.title')}</h2>
    <p id="archive-password-description" class="description">
      {localeStore.t('archivePassword.description', { archive: archiveName })}
    </p>

    <label for="archive-password-input">{localeStore.t('archivePassword.label')}</label>
    <input
      id="archive-password-input"
      {@attach trackPasswordInput}
      bind:value={password}
      type="password"
      autocomplete="off"
      placeholder={localeStore.t('archivePassword.placeholder')}
      disabled={challenge?.isSubmitting ?? false}
      required
    />

    {#if challenge?.incorrectPassword}
      <p class="error" role="alert">{localeStore.t('archivePassword.incorrect')}</p>
    {/if}

    <div class="actions">
      <button type="button" class="secondary" onclick={cancel}>
        {localeStore.t('common.cancel')}
      </button>
      <button
        type="submit"
        class="primary"
        disabled={!password || (challenge?.isSubmitting ?? false)}
      >
        {challenge?.isSubmitting
          ? localeStore.t('archivePassword.unlocking')
          : localeStore.t('archivePassword.submit')}
      </button>
    </div>
  </form>
</dialog>

<style>
  .dialog {
    width: min(400px, calc(100% - 2 * var(--space-4)));
    max-width: none;
    padding: 0;
    display: flex;
    background: var(--color-bg);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-overlay);
    color: var(--color-text);
  }

  .dialog:not([open]) {
    display: none;
  }

  .dialog::backdrop {
    background: rgba(0, 0, 0, 0.45);
  }

  .dialog-content {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-5);
  }

  h2 {
    font-size: var(--text-lg);
    font-weight: 600;
    color: var(--color-text);
  }

  .description {
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  label {
    margin-bottom: calc(-1 * var(--space-2));
    color: var(--color-text-secondary);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  input {
    width: 100%;
    padding: var(--space-2) var(--space-3);
  }

  .error {
    padding: var(--space-2);
    color: var(--color-error);
    background: var(--color-error-bg);
    border-radius: var(--radius-sm);
    font-size: var(--text-sm);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-1);
  }

  .actions button {
    min-width: 88px;
    padding: var(--space-2) var(--space-3);
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .secondary {
    background: var(--color-surface-alt);
    border: 1px solid var(--color-border);
  }

  .secondary:hover {
    background: var(--color-border-light);
  }

  .primary {
    color: #fff;
    background: var(--color-accent);
  }

  .primary:hover:not(:disabled) {
    background: var(--color-accent-hover);
  }
</style>
