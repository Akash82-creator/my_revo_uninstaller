<script lang="ts">
  import { formatDate, type Preview } from './api';

  let { details, note }: { details: Preview; note?: string } = $props();
  const p = $derived(details.program);
</script>

<div class="panel top">
  <h2>{p.name}</h2>
  {#if note}<p class="muted">{note}</p>{/if}
  <dl>
    <dt>Publisher</dt><dd>{p.publisher ?? '—'}</dd>
    <dt>Version</dt><dd>{p.version ?? '—'}</dd>
    <dt>Installed for</dt><dd>{p.scope} ({p.views.join(', ')})</dd>
    <dt>Install date</dt><dd>{formatDate(p.installDate) || '—'}</dd>
    <dt>Install folder</dt>
    <dd>
      {#if details.installDir}<code>{details.installDir}</code>
      {:else}<span class="muted">not used as evidence ({details.installDirRejected})</span>{/if}
    </dd>
    <dt>Uninstall command</dt>
    <dd>
      {#if details.commandSummary}<code>{details.commandSummary}</code>
      {:else}<span class="bad-text">Not supported: {details.commandError}</span>{/if}
    </dd>
  </dl>
</div>

<style>
  .top { margin-top: 12px; }
  h2 { margin: 0 0 8px; font-size: 18px; }
  dl { display: grid; grid-template-columns: 140px 1fr; gap: 4px 12px; margin: 0; }
  dt { color: var(--muted); }
  dd { margin: 0; }
  .bad-text { color: var(--bad); }
</style>
