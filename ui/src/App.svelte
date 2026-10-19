<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { api, onUninstallUpdate, type EnvStatus, type OpView, type Program } from './api';
  import ProgramList from './ProgramList.svelte';
  import OperationPanel from './OperationPanel.svelte';
  import ProgramPanel from './ProgramPanel.svelte';
  import Review from './Review.svelte';

  let env = $state<EnvStatus | null>(null);
  let programs = $state<Program[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let selected = $state<string | null>(null);
  let selectedOp = $state<string | null>(null);
  let scanId = $state<string | null>(null);
  // Uninstall operations live here, not in ProgramPanel, so navigating away
  // never loses track of an uninstaller that is still running.
  let ops = $state<OpView[]>([]);
  let unlisten: (() => void) | null = null;

  const stateText: Record<string, string> = {
    ready: 'preparing', launching: 'starting', running: 'running', verifying: 'checking result', finished: 'finished',
  };
  const running = $derived(ops.filter((o) => o.state !== 'finished'));
  const currentOp = $derived(ops.find((o) => o.opId === selectedOp) ?? null);

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
    selectedOp = null;
    refresh();
  }

  function open(programId: string) {
    scanId = null;
    selectedOp = null;
    selected = programId;
  }

  function openOp(opId: string) {
    scanId = null;
    selected = null;
    selectedOp = opId;
  }

  function started(op: OpView) {
    upsert(op);
    openOp(op.opId);
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
    {#if selectedOp !== o.opId}
      <div class="banner warn running" data-testid="running-op">
        Uninstaller for <strong>{o.programName}</strong> is still running.
        <button onclick={() => openOp(o.opId)}>Open</button>
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
  {:else if currentOp}
    {#key currentOp.opId}
      <OperationPanel op={currentOp} onBack={back} onScan={(id) => (scanId = id)} />
    {/key}
  {:else if selected}
    {#key selected}
      <ProgramPanel programId={selected} onStarted={started} onBack={back} onScan={(id) => (scanId = id)} />
    {/key}
  {:else}
    {#if ops.length > 0}
      <section class="panel" data-testid="recent-ops">
        <h3>Recent uninstalls</h3>
        <ul class="ops">
          {#each ops as o (o.opId)}
            <li>
              <strong>{o.programName}</strong>
              <span class="muted">{o.outcome ? o.outcome.label : stateText[o.state]}</span>
              <button onclick={() => openOp(o.opId)}>Open</button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
    <ProgramList {programs} {loading} onSelect={open} onRefresh={refresh} />
  {/if}
</main>

<style>
  header { display: flex; align-items: baseline; gap: 12px; padding: 12px 20px; border-bottom: 1px solid var(--border); background: var(--panel); }
  h1 { font-size: 18px; margin: 0; }
  main { padding: 14px 20px; max-width: 1200px; margin: 0 auto; }
  .running { display: flex; gap: 10px; align-items: center; }
  h3 { margin: 0 0 8px; font-size: 15px; }
  .ops { list-style: none; margin: 0; padding: 0; }
  .ops li { display: flex; gap: 10px; align-items: center; padding: 4px 0; border-top: 1px solid var(--border); }
  .ops li:first-child { border-top: none; }
</style>
