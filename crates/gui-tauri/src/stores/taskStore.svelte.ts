// Task progress state store using Svelte 5 module-level $state runes.

import type { TaskProgressPayload } from '../bridge';

interface TaskState {
  taskId: string;
  kind: 'compress' | 'extract';
  status: 'pending' | 'running' | 'finished' | 'cancelled' | 'failed';
  stage: string;
  message: string;
  percent: number | null;
  current: number;
  total: number | null;
  bytesPerSecond: number | null;
  currentEntry: string | null;
  completedEntries: number;
  totalEntries: number | null;
}

// --- Reactive State ---
let activeTask: TaskState | null = $state(null);

let isRunning = $derived(
  activeTask !== null &&
    (activeTask.status === 'pending' || activeTask.status === 'running')
);

// --- Functions ---

function startTask(taskId: string, kind: 'compress' | 'extract'): void {
  activeTask = {
    taskId,
    kind,
    status: 'pending',
    stage: 'scanning',
    message: '',
    percent: null,
    current: 0,
    total: null,
    bytesPerSecond: null,
    currentEntry: null,
    completedEntries: 0,
    totalEntries: null,
  };
}

function updateTask(payload: TaskProgressPayload): void {
  if (!activeTask) return;

  if (payload.task_id) {
    activeTask.taskId = payload.task_id;
  }
  if (payload.kind) {
    activeTask.kind = payload.kind;
  }
  if (payload.status) {
    activeTask.status = payload.status;
  }
  if (payload.stage) {
    activeTask.stage = payload.stage;
  }
  if (payload.message !== undefined) {
    activeTask.message = payload.message;
  }
  if (payload.percent !== undefined) {
    activeTask.percent = payload.percent;
  }
  if (payload.current !== undefined) {
    activeTask.current = payload.current;
  }
  if (payload.total !== undefined) {
    activeTask.total = payload.total;
  }
  if (payload.bytes_per_second !== undefined) {
    activeTask.bytesPerSecond = payload.bytes_per_second;
  }
  if (payload.current_entry !== undefined) {
    activeTask.currentEntry = payload.current_entry;
  }
  if (payload.completed_entries !== undefined) {
    activeTask.completedEntries = payload.completed_entries;
  }
  if (payload.total_entries !== undefined) {
    activeTask.totalEntries = payload.total_entries;
  }
}

function finishTask(
  status: 'finished' | 'cancelled' | 'failed',
  message?: string
): void {
  if (activeTask) {
    activeTask.status = status;
    if (message !== undefined) {
      activeTask.message = message;
    }
  }
}

function resetTask(): void {
  activeTask = null;
}

// --- Export ---
export const taskStore = {
  get activeTask() {
    return activeTask;
  },
  get isRunning() {
    return isRunning;
  },
  startTask,
  updateTask,
  finishTask,
  resetTask,
};
