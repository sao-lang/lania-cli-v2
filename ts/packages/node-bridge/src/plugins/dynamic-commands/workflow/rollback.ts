import type { DynamicCommandContext } from '../types.js';
import type { WorkflowExecutionState } from './state.js';

// File rollback preparation is intentionally stateful:
// - the first time we see a file we snapshot its original contents / absence
// - subsequent writes in the same workflow should still roll back to that original state

export async function createFileRollbackHook(
  filePath: string,
  ctx: DynamicCommandContext,
  state: WorkflowExecutionState,
): Promise<() => Promise<void>> {
  if (state.preparedRollbackFiles.has(filePath)) {
    const existed = await ctx.tools.fs.exists(filePath);
    if (existed) {
      const previousContent = await ctx.tools.fs.read(filePath);
      return async () => {
        await ctx.tools.fs.write(filePath, previousContent, { mkdirp: true });
      };
    }
    return async () => {
      await ctx.tools.fs.remove(filePath, { recursive: false });
    };
  }

  state.preparedRollbackFiles.add(filePath);
  const existed = await ctx.tools.fs.exists(filePath);
  if (existed) {
    const previousContent = await ctx.tools.fs.read(filePath);
    return async () => {
      await ctx.tools.fs.write(filePath, previousContent, { mkdirp: true });
    };
  }

  return async () => {
    await ctx.tools.fs.remove(filePath, { recursive: false });
  };
}

