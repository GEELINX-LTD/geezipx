// Reactive i18n wrapper around the existing i18n module.
// Uses a counter-based reactivity trick so that `t()` calls re-run
// when the locale changes.

import { getLocale, setLocale, t as tFn, type Locale } from '../i18n';

let localeVersion = $state(0);

function switchLocale(locale: Locale): void {
  setLocale(locale);
  localeVersion++;
}

function t(key: string, params?: Record<string, string | number>): string {
  void localeVersion; // track dependency — re-runs deriveds/effects on change
  return tFn(key, params);
}

export const localeStore = {
  get locale() {
    return getLocale();
  },
  switchLocale,
  t,
};
