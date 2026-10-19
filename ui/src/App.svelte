<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { api, onUninstallUpdate, type EnvStatus, type OpView, type Program } from './api';
  import ProgramList from './ProgramList.svelte';
  import ProgramPanel from './ProgramPanel.svelte';
  import Review from './Review.svelte';

  let env = $state<EnvStatus | null>(null);
  let programs = $state<Program[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let selected = $state<string | null>(null);
  let scanId = $state<string | null>(null);
  // Uninstall operations live here, not in ProgramPanel, so navigating away
  // never loses track of an uninstaller that is still running.
  let ops = $state<OpView[]>([]);
  let unlisten: (() => void) | null = null;

  const running = $derived(ops.filter((o) => o.state !== 'finished'));

  function upsert(v: OpView) {
    const i = ops.findIndex((o) => o.opId === v.opId);
    if (i >= 0) ops[i] = v;
    else ops = [v, ...ops];
  }

  async function refresh() {
    loading = true;
    error = null;
    try {
      programs = await api.listPrograms();
      env = await api.envStatus();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(async () => {
    unlisten = await onUninstallUpdate(upsert);
    try {
      // Re-attach to operations started before a reload.
      for (const o of await api.listOperations()) upsert(o);
    } catch (e) {
      error = String(e);
    }
    refresh();
  });
  onDestroy(() => unlisten?.());

  function back() {
    scanId = null;
    selected = null;
    refresh();
  }

  function open(programId: string) {
    scanId = null;
    selected = programId;
  }
</script>

<header>
  <h1>Sweep</h1>
  <span class="muted">Stage {env?.stage ?? 'A'} · evidence-based uninstaller</span>
</header>

<main>
  <div class="banner info" data-testid="stage-banner">
    Sweep's own cleanup is <strong>disabled in this build</strong>. Leftover scans are read-only.
    Vendor uninstallers you start <strong>do run and change your system</strong>.
  </div>
  {#each running as o (o.opId)}
    {#if selected !== o.programId}
      <div class="banner warn running" data-testid="running-op">
        Uninstaller for <strong>{o.programName}</strong> is still running.
        <button onclick={() => open(o.programId)}>Open</button>
      </div>
    {/if}
  {/each}
  {#if env?.elevated}
    <div class="banner warn">Sweep is running as administrator. Restart it normally: per-user uninstallers are refused while elevated.</div>
  {/if}
  {#if env && !env.supportedPlatform}
    <div class="banner bad">This platform is not supported; Sweep only works on Windows.</div>
  {/if}
  {#if env && env.inventoryUnreadable.length > 0}
    <div class="banner warn">Some registry locations could not be read, so the list may be incomplete:
      <ul>{#each env.inventoryUnreadable as u}<li class="mono">{u}</li>{/each}</ul>
    </div>
  {/if}
  {#if error}<div class="banner bad">{error}</div>{/if}

  {#if scanId}
    <Review {scanId} onBack={back} />
  {:else if selected}
    {#key selected}
      <ProgramPanel
        programId={selected}
        op={ops.find((o) => o.programId === selected) ?? null}
        onStarted={upsert}
        onBack={back}
        onScan={(id) => (scanId = id)}
      />
    {/key}
  {:else}
    <ProgramList {programs} {loading} onSelect={open} onRefresh={refresh} />
  {/if}
</main>

<style>
  header { display: flex; align-items: baseline; gap: 12px; padding: 12px 20px; border-bottom: 1px solid var(--border); background: var(--panel); }
  h1 { font-size: 18px; margin: 0; }
  main { padding: 14px 20px; max-width: 1200px; margin: 0 auto; }
  .running { display: flex; gap: 10px; align-items: center; }
</style>
