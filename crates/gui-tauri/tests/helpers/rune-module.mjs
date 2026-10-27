import fs from 'node:fs/promises';
import ts from 'typescript';
import { compileModule } from 'svelte/compiler';

let instance = 0;

// Exercise Svelte's real client runtime; no stand-in rune functions are used.
export async function runeModule(relativePath, imports = {}) {
  const sourceUrl = new URL(`../../src/${relativePath}`, import.meta.url);
  const source = await fs.readFile(sourceUrl, 'utf8');
  const js = ts.transpileModule(source, {
    fileName: sourceUrl.pathname,
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
  }).outputText;
  const compiled = compileModule(js, { filename: sourceUrl.pathname, generate: 'client' });
  if (compiled.warnings.length) throw new Error(JSON.stringify(compiled.warnings));
  const bindingsKey = `__rune_test_imports_${++instance}`;
  globalThis[bindingsKey] = imports;
  const code = compiled.js.code
    .replace(/import\s*\{([^}]+)\}\s*from\s*(['"])(.*?)\2;?/g, (statement, names, _quote, specifier) => {
      if (Object.hasOwn(imports, specifier)) {
        return `const {${names.replace(/\s+as\s+/g, ': ')}} = globalThis[${JSON.stringify(bindingsKey)}][${JSON.stringify(specifier)}];`;
      }
      return statement;
    })
    .replace(/(['"])svelte\/internal\/client\1/g, JSON.stringify(import.meta.resolve('svelte/internal/client')))
    .replace(/(['"])svelte\/reactivity\1/g, JSON.stringify(new URL('src/reactivity/index-client.js', import.meta.resolve('svelte/package.json')).href));
  try {
    return await import(`data:text/javascript;base64,${Buffer.from(`${code}\n// instance ${instance}`).toString('base64')}`);
  } finally {
    delete globalThis[bindingsKey];
  }
}
