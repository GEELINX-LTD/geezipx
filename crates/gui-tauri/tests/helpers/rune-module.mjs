import fs from 'node:fs/promises';
import ts from 'typescript';
import { compileModule } from 'svelte/compiler';

let instance = 0;

// Exercise Svelte's real client runtime; no stand-in rune functions are used.
export async function runeModule(relativePath) {
  const sourceUrl = new URL(`../../src/${relativePath}`, import.meta.url);
  const source = await fs.readFile(sourceUrl, 'utf8');
  const js = ts.transpileModule(source, {
    fileName: sourceUrl.pathname,
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
  }).outputText;
  const compiled = compileModule(js, { filename: sourceUrl.pathname, generate: 'client' });
  if (compiled.warnings.length) throw new Error(JSON.stringify(compiled.warnings));
  const runtimeUrl = import.meta.resolve('svelte/internal/client');
  const code = compiled.js.code.replace(/(['"])svelte\/internal\/client\1/g, JSON.stringify(runtimeUrl));
  return import(`data:text/javascript;base64,${Buffer.from(`${code}\n// instance ${++instance}`).toString('base64')}`);
}
