import en from "./locales/en.json";
import zhCN from "./locales/zh-CN.json";
import type { Locale, MessageValue, Messages } from "./types";
export type { Locale } from "./types";

const STORAGE_KEY = "geezipx_locale";
const DEFAULT_LOCALE: Locale = "zh-CN";
const SUPPORTED_LOCALES: Locale[] = ["zh-CN", "en"];
export const LOCALE_CHANGED_EVENT = "geezipx:locale-changed";

const dictionaries: Record<Locale, Messages> = {
  "zh-CN": zhCN as Messages,
  en: en as Messages,
};

let currentLocale: Locale = resolveInitialLocale();

function isLocale(value: string): value is Locale {
  return SUPPORTED_LOCALES.includes(value as Locale);
}

function normalizeLocale(value?: string | null): Locale {
  if (!value) {
    return DEFAULT_LOCALE;
  }

  if (isLocale(value)) {
    return value;
  }

  const lower = value.toLowerCase();
  if (lower.startsWith("en")) {
    return "en";
  }
  if (lower.startsWith("zh")) {
    return "zh-CN";
  }
  return DEFAULT_LOCALE;
}

function getStoredLocale(): Locale | null {
  if (typeof window === "undefined") {
    return null;
  }

  try {
    const value = window.localStorage.getItem(STORAGE_KEY);
    return value && isLocale(value) ? value : null;
  } catch {
    return null;
  }
}

function detectNavigatorLocale(): Locale {
  if (typeof navigator === "undefined") {
    return DEFAULT_LOCALE;
  }

  const preferred = navigator.language || navigator.languages?.[0] || null;
  return normalizeLocale(preferred);
}

function resolveInitialLocale(): Locale {
  return getStoredLocale() ?? detectNavigatorLocale();
}

function resolveMessage(messages: Messages, key: string): string | null {
  const parts = key.split(".");
  let current: MessageValue | undefined = messages;

  for (const part of parts) {
    if (!current || typeof current === "string" || !(part in current)) {
      return null;
    }
    current = current[part];
  }

  return typeof current === "string" ? current : null;
}

function interpolate(template: string, params?: Record<string, string | number>): string {
  return template.replace(/{{\s*([\w.-]+)\s*}}/g, (_, token: string) => {
    if (!params || !(token in params)) {
      return `{{${token}}}`;
    }
    return String(params[token]);
  });
}

function collectTranslatableElements(root: ParentNode): Element[] {
  const selector = "[data-i18n], [data-i18n-placeholder], [data-i18n-title]";
  const elements = new Set<Element>();

  if (root instanceof Element && root.matches(selector)) {
    elements.add(root);
  }

  if ("querySelectorAll" in root) {
    root.querySelectorAll(selector).forEach((element) => elements.add(element));
  }

  return [...elements];
}

function translateElement(element: Element): void {
  const textKey = element.getAttribute("data-i18n");
  if (textKey) {
    element.textContent = t(textKey);
  }

  const placeholderKey = element.getAttribute("data-i18n-placeholder");
  if (placeholderKey && element instanceof HTMLInputElement) {
    element.placeholder = t(placeholderKey);
  }

  const titleKey = element.getAttribute("data-i18n-title");
  if (titleKey) {
    element.setAttribute("title", t(titleKey));
  }
}

export function t(key: string, params?: Record<string, string | number>): string {
  const current = resolveMessage(dictionaries[currentLocale], key);
  const fallback = resolveMessage(dictionaries[DEFAULT_LOCALE], key);
  const message = current ?? fallback ?? key;
  return interpolate(message, params);
}

export function getLocale(): Locale {
  return currentLocale;
}

export function setLocale(locale: Locale): void {
  currentLocale = normalizeLocale(locale);

  if (typeof window !== "undefined") {
    try {
      window.localStorage.setItem(STORAGE_KEY, currentLocale);
    } catch {
      // Ignore storage failures in preview or private mode.
    }
  }

  applyI18n();
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent(LOCALE_CHANGED_EVENT, { detail: { locale: currentLocale } }));
  }
}

export function applyI18n(root?: ParentNode): void {
  if (typeof document === "undefined") {
    return;
  }

  const target = root ?? document;
  document.documentElement.lang = currentLocale;
  for (const element of collectTranslatableElements(target)) {
    translateElement(element);
  }
}
