<script lang="ts">
  import DropZone from './home/DropZone.svelte';
  import ActionCards from './home/ActionCards.svelte';
  import RecentFiles from './home/RecentFiles.svelte';
</script>

<div class="home-page">
  <DropZone />
  <ActionCards />
  <RecentFiles />
</div>

<style>
  .home-page {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-6);
    padding: var(--space-8) var(--space-6);
    overflow-y: auto;
    flex: 1;
  }
</style>
