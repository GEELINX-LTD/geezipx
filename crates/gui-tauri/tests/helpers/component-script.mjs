// Test command handlers from the actual component script without a desktop webview.
// Native APIs are injected; DOM rendering and Svelte effects are covered by build.
import fs from 'node:fs';
import ts from 'typescript';

export function componentScript(relativePath, bindings, expose) {
  const source = fs.readFileSync(new URL(`../../src/${relativePath}`, import.meta.url), 'utf8');
  const script = source.match(/<script[^>]*>([\s\S]*?)<\/script>/)?.[1];
  if (!script) throw new Error(`No component script: ${relativePath}`);
  const js = ts.transpileModule(script, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
  }).outputText.replace(/^import\s[\s\S]*?;\s*$/gm, '');
  const defaults = {
    $state: Object.assign(value => value, { raw: value => value }),
    $derived: Object.assign(value => value, { by: fn => fn() }),
    $effect: () => {},
    onDestroy: () => {},
    untrack: fn => fn(),
  };
  const values = { ...defaults, ...bindings };
  return new Function(...Object.keys(values), `${js}\nreturn (${expose});`)(...Object.values(values));
}

export async function settle() {
  for (let i = 0; i < 20; i++) await Promise.resolve();
}

export function deferred() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}
