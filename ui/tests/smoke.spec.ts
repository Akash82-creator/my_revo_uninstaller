// UI smoke tests against a MOCKED backend (window.__TAURI_INTERNALS__).
// These verify UI behaviour only; they say nothing about Windows behaviour.
import { expect, test, type Page } from '@playwright/test';

type Scenario = 'success' | 'cancelled' | 'manual';

async function mockBackend(page: Page, scenario: Scenario) {
  await page.addInitScript((scenario: Scenario) => {
    const callbacks = new Map<number, (e: unknown) => void>();
    const listeners: Record<string, number[]> = {};
    let next = 1;
    const calls: Array<{ cmd: string; args: unknown }> = [];
    (window as any).__calls = calls;
    const emit = (event: string, payload: unknown) =>
      (listeners[event] ?? []).forEach((id) => callbacks.get(id)?.({ event, id, payload }));
    (window as any).__emit = emit;
    // Backend-side state change: update the stored operation and emit it.
    (window as any).__update = (patch: any) => {
      const i = ops.findIndex((o) => o.opId === 'op-1');
      ops[i] = { ...ops[i], ...patch };
      saveOps();
      emit('uninstall-update', ops[i]);
    };
    // Operations persist across reloads like backend state does.
    const ops: any[] = JSON.parse(sessionStorage.getItem('mockOps') ?? '[]');
    const saveOps = () => sessionStorage.setItem('mockOps', JSON.stringify(ops));
    // In the 'manual' scenario the uninstaller removes the program's
    // registration as soon as it starts, like real uninstallers do.
    const isRemoved = () => sessionStorage.getItem('mockRemoved') === '1';

    const program = (id: string, name: string) => ({
      id, name, publisher: 'Acme Corporation', version: '2.3', scope: 'All users', views: ['64-bit'],
      msi: false, sizeKb: 2048, installDate: '20240101', hasUninstaller: true,
    });
    const report = {
      complete: false, cancelled: false, uncertain: scenario !== 'success', cleanupEnabled: false, itemsVisited: 42,
      coverage: [
        { location: 'Program Files: C:\\Program Files', status: 'scanned', detail: null, gap: false },
        { location: 'ProgramData: C:\\ProgramData', status: 'access denied', detail: null, gap: true },
      ],
      candidates: [
        { id: 'c0', target: 'C:\\Program Files\\Acme\\Notes', kind: 'folder', verdict: 'likely', discoveredBy: ["program's InstallLocation"],
          ownership: ["is the program's validated InstallLocation"], shared: [], userData: [], concerns: [], protection: null,
          stats: { files: 2, dirs: 1, bytes: 4096 } },
        { id: 'c1', target: 'C:\\Users\\alice\\AppData\\Roaming\\Acme', kind: 'folder', verdict: 'review',
          discoveredBy: ['name equals the publisher'], ownership: [], shared: ['publisher shared with 1 other installed program(s)'],
          userData: [], concerns: [], protection: null, stats: null },
        { id: 'c2', target: 'C:\\Program Files\\Acme', kind: 'folder', verdict: 'blocked', discoveredBy: ['name equals the publisher'],
          ownership: [], shared: [], userData: [], concerns: [], protection: 'contains protected area: installed program: Acme Paint', stats: null },
      ],
    };
    const preview = (id: string) => ({
      program: program(id, 'Acme Notes'), installDir: 'C:\\Program Files\\Acme\\Notes', installDirRejected: null,
      commandSummary: '"C:\\Program Files\\Acme\\Notes\\unins000.exe" /SILENT', commandError: null,
      plan: { verb: 'runas', file: 'C:\\Program Files\\Acme\\Notes\\unins000.exe', params: '/SILENT',
        reason: 'Per-machine program; Windows will ask for administrator approval for the uninstaller only.',
        elevation: 'Windows will ask for administrator approval (for the uninstaller only)' },
      refusal: null, restorePoint: 'unavailable', restorePointNote: 'Unavailable in this build.', canLaunch: true,
    });
    const opFinished = {
      opId: 'op-1', programId: 'm64:AcmeNotes', programName: 'Acme Notes',
      program: program('m64:AcmeNotes', 'Acme Notes'), launch: preview('m64:AcmeNotes'),
      state: 'finished', pids: [], trackingIncomplete: false,
      outcome: scenario === 'success'
        ? { kind: 'succeeded', label: 'Uninstaller finished', detail: '', autoScanAllowed: true }
        : { kind: 'cancelled', label: 'Uninstall was cancelled', detail: '', autoScanAllowed: false },
    };

    const handlers: Record<string, (a: any) => unknown> = {
      env_status: () => ({ stage: 'A', cleanupEnabled: false, restorePoint: 'unavailable', elevated: false, supportedPlatform: true, inventoryUnreadable: [] }),
      list_programs: () =>
        [program('m64:AcmeNotes', 'Acme Notes'), program('m64:AcmePaint', 'Acme Paint')].filter(
          (p) => !(isRemoved() && p.id === 'm64:AcmeNotes'),
        ),
      preview_uninstall: (a) => {
        if (isRemoved() && a.programId === 'm64:AcmeNotes') throw 'unknown program; refresh the list';
        return preview(a.programId);
      },
      start_uninstall: () => {
        if (ops.some((o) => o.state !== 'finished')) throw 'Another uninstaller is still running.';
        ops.push({ ...opFinished, state: 'running', pids: [1234], outcome: null });
        saveOps();
        if (scenario === 'manual') sessionStorage.setItem('mockRemoved', '1');
        else setTimeout(() => emit('uninstall-update', opFinished), 50);
        return 'op-1';
      },
      get_operation: () => ops.find((o) => o.opId === 'op-1'),
      list_operations: () => ops,
      start_scan: (a) => {
        if (scenario === 'cancelled' && !a.acknowledgeUncertain) throw 'confirm required';
        setTimeout(() => emit('scan-done', { scanId: 'scan-1', phase: 'Done', itemsVisited: 42, candidateCount: 3, done: true, report }), 50);
        return 'scan-1';
      },
      get_scan: () => ({ scanId: 'scan-1', phase: 'Scanning Program Files', itemsVisited: 10, candidateCount: 1, done: false, report: null }),
      cancel_scan: () => null,
      stop_waiting: () => null,
      'plugin:event|listen': (a) => {
        (listeners[a.event] ??= []).push(a.handler);
        return a.handler;
      },
      'plugin:event|unlisten': () => null,
    };

    (window as any).__TAURI_INTERNALS__ = {
      transformCallback: (cb: (e: unknown) => void) => {
        const id = next++;
        callbacks.set(id, cb);
        return id;
      },
      invoke: async (cmd: string, args: unknown) => {
        calls.push({ cmd, args });
        const h = handlers[cmd];
        if (!h) throw `unmocked command ${cmd}`;
        return h(args);
      },
      metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main', windowLabel: 'main' } },
    };
  }, scenario);
}

test('success path: uninstall, auto read-only scan, evidence review without delete controls', async ({ page }) => {
  await mockBackend(page, 'success');
  await page.goto('/');
  await expect(page.getByTestId('stage-banner')).toContainText('disabled in this build');
  await expect(page.getByTestId('program-row')).toHaveCount(2);
  await page.getByRole('button', { name: 'Acme Notes' }).click();

  await expect(page.getByTestId('restore-point')).toContainText('unavailable');
  const run = page.getByRole('button', { name: 'Run vendor uninstaller' });
  await expect(run).toBeDisabled();
  await page.getByLabel(/vendor's uninstaller will run/).check();
  await run.click();

  // Success → scan starts automatically → review.
  await expect(page.getByTestId('cleanup-disabled')).toBeVisible();
  await expect(page.getByTestId('coverage-incomplete')).toContainText('access denied');
  await expect(page.getByTestId('group-likely')).toContainText('C:\\Program Files\\Acme\\Notes');
  await expect(page.getByTestId('group-review')).toContainText('AppData\\Roaming\\Acme');
  await expect(page.getByTestId('group-blocked')).toContainText('C:\\Program Files\\Acme');

  // Expand evidence for a name-only candidate.
  await page.getByRole('button', { name: /AppData\\Roaming\\Acme/ }).click();
  await expect(page.getByTestId('group-review')).toContainText('none (name match only)');

  // No destructive controls anywhere.
  await expect(page.getByRole('button', { name: /delete|remove|clean/i })).toHaveCount(0);
  await expect(page.getByRole('checkbox')).toHaveCount(0);

  // The frontend only ever sent IDs, never paths or commands.
  const calls = await page.evaluate(() => (window as any).__calls as Array<{ cmd: string; args: any }>);
  const sent = JSON.stringify(calls.filter((c) => !c.cmd.startsWith('plugin:')).map((c) => c.args));
  expect(sent).not.toMatch(/\\\\|\.exe|C:/);
});

test('cancelled uninstall: no automatic scan; explicit acknowledgement required', async ({ page }) => {
  await mockBackend(page, 'cancelled');
  await page.goto('/');
  await page.getByRole('button', { name: 'Acme Notes' }).click();
  await page.getByLabel(/vendor's uninstaller will run/).check();
  await page.getByRole('button', { name: 'Run vendor uninstaller' }).click();

  await expect(page.getByTestId('outcome')).toContainText('cancelled');
  const scanAnyway = page.getByRole('button', { name: 'Scan anyway (read-only)' });
  await expect(scanAnyway).toBeDisabled();
  const scansBefore = await page.evaluate(() => (window as any).__calls.filter((c: any) => c.cmd === 'start_scan').length);
  expect(scansBefore).toBe(0);
  await page.getByLabel(/Scan anyway; I understand/).check();
  await scanAnyway.click();
  await expect(page.getByText('The uninstall outcome was not a confirmed success')).toBeVisible();
});

async function startManual(page: Page) {
  await mockBackend(page, 'manual');
  await page.goto('/');
  await page.getByRole('button', { name: 'Acme Notes' }).click();
  await page.getByLabel(/vendor's uninstaller will run/).check();
  await page.getByRole('button', { name: 'Run vendor uninstaller' }).click();
  await expect(page.getByTestId('operation')).toContainText('Uninstaller running');
}

const finished = {
  state: 'finished', pids: [], trackingIncomplete: false,
  outcome: { kind: 'succeeded', label: 'Uninstaller finished', detail: '', autoScanAllowed: true },
};

test('operation stays reachable after its registration disappears', async ({ page }) => {
  await startManual(page);
  // Leave while running; the inventory refresh no longer lists the program.
  await page.getByRole('button', { name: '← Programs' }).click();
  await expect(page.getByTestId('program-row')).toHaveCount(1);
  await expect(page.getByRole('button', { name: 'Acme Notes' })).toHaveCount(0);
  await expect(page.getByTestId('running-op')).toContainText('Acme Notes');

  // Progress while away is not lost; reopening needs no live inventory.
  await page.evaluate(() => (window as any).__update({ pids: [1234, 5678] }));
  await page.getByTestId('running-op').getByRole('button', { name: 'Open' }).click();
  await expect(page.getByTestId('operation')).toContainText('Tracking 2 process(es)');
  await expect(page.getByRole('button', { name: 'Stop waiting' })).toBeVisible();
  await expect(page.getByText('unknown program')).toHaveCount(0);
  await expect(page.getByText('unins000.exe').first()).toBeVisible();

  // Outcome arrives while the panel is open: read-only scan starts.
  await page.evaluate((f) => (window as any).__update(f), finished);
  await expect(page.getByTestId('cleanup-disabled')).toBeVisible();
});

test('operation that finishes while away stays in history', async ({ page }) => {
  await startManual(page);
  await page.getByRole('button', { name: '← Programs' }).click();
  await page.evaluate((f) => (window as any).__update(f), finished);
  await expect(page.getByTestId('running-op')).toHaveCount(0);
  const recent = page.getByTestId('recent-ops');
  await expect(recent).toContainText('Acme Notes');
  await expect(recent).toContainText('Uninstaller finished');
  await recent.getByRole('button', { name: 'Open' }).click();
  await expect(page.getByTestId('outcome')).toContainText('Uninstaller finished');
  // Finished before it was opened: no automatic scan, an explicit button instead.
  const scans = await page.evaluate(() => (window as any).__calls.filter((c: any) => c.cmd === 'start_scan').length);
  expect(scans).toBe(0);
  await page.getByRole('button', { name: 'Scan for leftovers (read-only)' }).click();
  await expect(page.getByTestId('cleanup-disabled')).toBeVisible();
  const scanArgs = await page.evaluate(() => (window as any).__calls.filter((c: any) => c.cmd === 'start_scan').map((c: any) => c.args));
  expect(scanArgs).toEqual([{ opId: 'op-1', programId: null, acknowledgeUncertain: false }]);
});

test('reload re-attaches to a running operation', async ({ page }) => {
  await startManual(page);
  await page.reload();
  // Component state is gone and the program is gone from inventory; the
  // banner comes from list_operations.
  await expect(page.getByTestId('running-op')).toContainText('Acme Notes');
  await page.getByTestId('running-op').getByRole('button', { name: 'Open' }).click();
  await expect(page.getByTestId('operation')).toContainText('Uninstaller running');
  await expect(page.getByRole('button', { name: 'Run vendor uninstaller' })).toHaveCount(0);
});
