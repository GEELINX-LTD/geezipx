// Toast notification store using Svelte 5 module-level $state runes.

// --- Types ---

export interface Toast {
  id: number;
  message: string;
  type: 'success' | 'error' | 'info';
  duration: number;
}

// --- Reactive State ---

let toasts = $state<Toast[]>([]);

// --- Internal ---

let nextId = 0;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

// --- Functions ---

function show(
  message: string,
  type: Toast['type'] = 'info',
  duration: number = 3000,
): void {
  const id = nextId++;
  const toast: Toast = { id, message, type, duration };
  toasts = [...toasts, toast];

  if (duration > 0) {
    const timer = setTimeout(() => dismiss(id), duration);
    timers.set(id, timer);
  }
}

function dismiss(id: number): void {
  const timer = timers.get(id);
  if (timer) {
    clearTimeout(timer);
    timers.delete(id);
  }
  toasts = toasts.filter((t) => t.id !== id);
}

// --- Export ---

export const toastStore = {
  get toasts() {
    return toasts;
  },
  show,
  dismiss,
};
