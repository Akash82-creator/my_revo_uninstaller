<script lang="ts">
  import { api, type OpView } from './api';
  import ProgramDetails from './ProgramDetails.svelte';

  // Rendered entirely from the operation's own data: the program may already
  // have vanished from the live inventory.
  let { op, onBack, onScan }: { op: OpView; onBack: () => void; onScan: (scanId: string) => void } = $props();

  let error = $state<string | null>(null);
  let ackUncertain = $state(false);
  // Auto-scan only when the outcome arrives while this panel is open, not
  // when revisiting an operation that finished earlier.
  let sawUnfinished = false;
  let autoScanStarted = $state(false);

  $effect(() => {
    if (op.state !== 'finished') sawUnfinished = true;
    else if (sawUnfinished && op.outcome?.autoScanAllowed && !autoScanStarted) {
      autoScanStarted = true;
      scan(false);
    }
  });

  async function scan(ack: boolean) {
    try {
      onScan(await api.startScan({ opId: op.opId }, ack));
    } catch (e) {
      error = String(e);
    }
  }

  async function stopWaiting() {
    try {
      await api.stopWaiting(op.opId);
    } catch (e) {
      error = String(e);
    }
  }

  const stateLabel: Record<string, string> = {
    ready: 'Preparing', launching: 'Starting the uninstaller…', running: 'Uninstaller running', verifying: 'Checking the result…', finished: 'Finished',
  };
</script>

<button onclick={onBack}>← Programs</button>
{#if error}<div class="banner bad top">{error}</div>{/if}

<ProgramDetails details={op.launch} note="As recorded when the uninstaller was started." />

<div class="panel" data-testid="operation">
  <h3>{stateLabel[op.state]}</h3>
  {#if op.launch.plan}
    <p class="muted">Launched: <code>{op.launch.plan.file}</code> {op.launch.plan.params} · {op.launch.plan.elevation}</p>
  {/if}
  {#if op.state === 'running' || op.state === 'launching'}
    <progress></progress>
    <p class="muted">Follow the uninstaller's own window. Tracking {op.pids.length} process(es).</p>
    <button onclick={stopWaiting}>Stop waiting</button>
    <span class="muted">Use this if the uninstaller opened something that stays open (e.g. a browser).</span>
  {/if}
  {#if op.trackingIncomplete}
    <div class="banner warn">Not every uninstaller process was followed to completion; the result cannot be confirmed.</div>
  {/if}
  {#if op.outcome}
    <div class="banner {op.outcome.kind === 'succeeded' ? 'ok' : op.outcome.kind === 'failed' ? 'bad' : 'warn'}" data-testid="outcome">
      <strong>{op.outcome.label}.</strong> {op.outcome.detail}
    </div>
    {#if op.outcome.autoScanAllowed}
      {#if autoScanStarted}
        <p class="muted">Starting the read-only leftover scan…</p>
      {:else}
        <div class="actions"><button class="primary" onclick={() => scan(false)}>Scan for leftovers (read-only)</button></div>
      {/if}
    {:else}
      <p>Because the outcome is not a confirmed success, Sweep does not scan automatically.</p>
      <label><input type="checkbox" bind:checked={ackUncertain} /> Scan anyway; I understand the program may still be installed.</label>
      <div class="actions"><button disabled={!ackUncertain} onclick={() => scan(true)}>Scan anyway (read-only)</button></div>
    {/if}
  {/if}
</div>

<style>
  .top { margin-top: 12px; }
  h3 { margin: 0 0 10px; font-size: 15px; }
  .actions { margin-top: 10px; display: flex; gap: 8px; }
  label { display: block; margin-top: 6px; }
</style>
