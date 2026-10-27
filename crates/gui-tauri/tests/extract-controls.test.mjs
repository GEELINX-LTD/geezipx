import assert from 'node:assert/strict';
import test from 'node:test';
import { componentScript, deferred, settle } from './helpers/component-script.mjs';

function controls() {
  const defaults = deferred();
  const instance = componentScript('pages/extract/ExtractControls.svelte', {
    archiveStore: { suggestedOutputDir: '/suggested', archivePath: '/a.zip', selectedCount: 0 },
    taskStore: { hasRunning: () => false },
    settingsStore: { get: async key => (await defaults.promise)[key] },
    open: async () => '/chosen',
  }, '{ browseOutput, get output() { return outputDir; } }');
  return { instance, defaults };
}

test('late extraction defaults cannot overwrite a manually chosen output directory', async () => {
  const { instance, defaults } = controls();
  await instance.browseOutput();
  defaults.resolve({ default_output_dir: '/saved-default', overwrite_strategy: 'skip', on_complete: 'nothing' });
  await settle();
  assert.equal(instance.output, '/chosen');
});

test('saved extraction output defaults still apply before a manual choice', async () => {
  const { instance, defaults } = controls();
  defaults.resolve({ default_output_dir: '/saved-default', overwrite_strategy: 'skip', on_complete: 'nothing' });
  await settle();
  assert.equal(instance.output, '/saved-default');
});
