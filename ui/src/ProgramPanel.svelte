<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type OpView, type Preview } from './api';
  import ProgramDetails from './ProgramDetails.svelte';

  // Preview and start only. Running and finished operations are shown by
  // OperationPanel from their own saved data.
  let { programId, onStarted, onBack, onScan }: {
    programId: string;
    onStarted: (op: OpView) => void;
    onBack: () => void;
    onScan: (scanId: string) => void;
  } = $props();

  let preview = $state<Preview | null>(null);
  let error = $state<string | null>(null);
  let understood = $state(false);
  let ackAnalyze = $state(false);
  let busy = $state(false);

  onMount(async () => {
    try {
      preview = await api.previewUninstall(programId);
    } catch (e) {
      error = String(e);
    }
  });

  async function run() {
    busy = true;
    error = null;
    try {
      const opId = await api.startUninstall(programId);
      onStarted(await api.getOperation(opId));
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function analyze() {
    try {
      onScan(await api.startScan({ programId }, true));
    } catch (e) {
      error = String(e);
    }
  }
</script>

<button onclick={onBack}>← Programs</button>

{#if error}<div class="banner bad top">{error}</div>{/if}

{#if preview}
  <ProgramDetails details={preview} />
  <div class="panel" data-testid="before-uninstall">
    <h3>Before you uninstall</h3>
    <div class="banner warn" data-testid="restore-point">
      <strong>Restore point: unavailable.</strong> {preview.restorePointNote}
    </div>
    {#if preview.plan}
      <p><strong>How it will run:</strong> {preview.plan.reason}</p>
      <p class="muted">Elevation: {preview.plan.elevation}. Sweep itself stays unelevated.</p>
      <label><input type="checkbox" bind:checked={understood} />
        I understand the vendor's uninstaller will run and change my system. Sweep cannot undo it.</label>
      <div class="actions">
        <button class="primary" disabled={!understood || busy || !preview.canLaunch} onclick={run}>Run vendor uninstaller</button>
      </div>
    {:else}
      <div class="banner bad">Sweep will not start this uninstaller: {preview.refusal}</div>
    {/if}
    <hr />
    <p class="muted">Or look for related files and registry keys without uninstalling. Results describe the current installation and are read-only.</p>
    <label><input type="checkbox" bind:checked={ackAnalyze} /> The program is still installed; I understand results are only indicative.</label>
    <div class="actions"><button disabled={!ackAnalyze} onclick={analyze}>Analyze without uninstalling</button></div>
  </div>
{:else if !error}
  <p class="muted top">Loading…</p>
{/if}

<style>
  .top { margin-top: 12px; }
  h3 { margin: 0 0 10px; font-size: 15px; }
  .actions { margin-top: 10px; display: flex; gap: 8px; }
  hr { border: none; border-top: 1px solid var(--border); margin: 16px 0; }
  label { display: block; margin-top: 6px; }
</style>
