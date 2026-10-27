// Task progress state shared across pages and shell actions.

import type { TaskKind, TaskProgressPayload } from '../bridge';

export interface TaskState {
  readonly taskId: string;
  readonly kind: TaskKind;
  readonly ordinal: number;
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

type TerminalStatus = 'finished' | 'cancelled' | 'failed';

// Replace the map and its task values together so every consumer sees one snapshot.
let tasks = $state.raw<ReadonlyMap<string, Readonly<TaskState>>>(new Map());
let activeTaskId = $state<string | null>(null);
let activeTask = $derived(activeTaskId ? tasks.get(activeTaskId) ?? null : null);
let taskList = $derived([...tasks.values()]);
let nextOrdinal = 0;

function isRunning(task: Readonly<TaskState>): boolean {
  return task.status === 'pending' || task.status === 'running';
}

function replaceTask(task: Readonly<TaskState>): void {
  tasks = new Map([...tasks, [task.taskId, task]]);
}

function startTask(kind: TaskKind): string {
  const taskId = crypto.randomUUID();
  replaceTask({
    taskId,
    kind,
    ordinal: ++nextOrdinal,
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
  });
  activeTaskId = taskId;
  return taskId;
}

function updateTask(payload: TaskProgressPayload): void {
  const task = tasks.get(payload.task_id);
  // Foreign events cannot register tasks, change their kind, or revive a result.
  if (!task || task.kind !== payload.kind || !isRunning(task)) return;

  const statusMap: Record<TaskProgressPayload['status'], TaskState['status']> = {
    started: task.status, // A delayed start cannot regress running to pending.
    progress: 'running',
    finished: 'finished',
    cancelled: 'cancelled',
    failed: 'failed',
  };
  const status = statusMap[payload.status];
  if (!status) return;
  replaceTask({
    ...task,
    status,
    stage: payload.stage ?? task.stage,
    message: payload.message ?? task.message,
    percent: payload.percent !== undefined ? payload.percent : task.percent,
    current: payload.current ?? task.current,
    total: payload.total !== undefined ? payload.total : task.total,
    bytesPerSecond: payload.bytes_per_second !== undefined ? payload.bytes_per_second : task.bytesPerSecond,
    currentEntry: payload.current_entry !== undefined ? payload.current_entry : task.currentEntry,
    completedEntries: payload.completed_entries ?? task.completedEntries,
    totalEntries: payload.total_entries !== undefined ? payload.total_entries : task.totalEntries,
  });
}

function finishTask(taskId: string, status: TerminalStatus, message?: string): TaskState['status'] | undefined {
  const task = tasks.get(taskId);
  if (!task) return undefined;
  // A terminal event is authoritative over the invoke promise's completion.
  if (!isRunning(task)) return task.status;
  if (status === 'failed' && message?.includes('Operation cancelled by user')) {
    status = 'cancelled';
  }
  replaceTask({ ...task, status, stage: status === 'finished' ? 'completed' : status, message: message ?? task.message });
  return status;
}

function selectTask(taskId: string): void {
  if (tasks.has(taskId)) activeTaskId = taskId;
}

function dismissTask(taskId: string): void {
  const task = tasks.get(taskId);
  if (!task || isRunning(task)) return;
  const remaining = new Map([...tasks].filter(([id]) => id !== taskId));
  tasks = remaining;
  if (activeTaskId === taskId) {
    activeTaskId = [...remaining.values()].find(isRunning)?.taskId ?? remaining.keys().next().value ?? null;
  }
}

export const taskStore = {
  get activeTask() { return activeTask; },
  get tasks() { return taskList; },
  get isVisible() { return tasks.size > 0; },
  getTask(taskId: string) { return tasks.get(taskId); },
  hasRunning(kind?: TaskKind) {
    return taskList.some(task => (!kind || task.kind === kind) && isRunning(task));
  },
  startTask,
  updateTask,
  finishTask,
  selectTask,
  dismissTask,
};
