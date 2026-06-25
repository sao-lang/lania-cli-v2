import type { BridgeEvent } from '../protocol/events.js';
import { normalizeRuntimeInput } from './template/runtime.js';
import {
  dependenciesResult,
  listResult,
  outputTasksResult,
  questionsResult,
  renderAddTemplateResult,
  renderLogEvent,
  renderResult,
} from './template/service.js';

export const templatePlugin = {
  name: 'template',
  methods: [
    'template.list',
    'template.getQuestions',
    'template.getDependencies',
    'template.getOutputTasks',
    'template.render',
    'addTemplate.render',
  ],
  async handle(method: string, params: Record<string, unknown>) {
    const runtimeInput = normalizeRuntimeInput(params);
    switch (method) {
      case 'template.list':
        return listResult(runtimeInput);
      case 'template.getQuestions':
        return questionsResult(params.template, runtimeInput);
      case 'template.getDependencies':
        return dependenciesResult(params.template, runtimeInput);
      case 'template.getOutputTasks':
        return outputTasksResult(params.template, runtimeInput);
      case 'template.render':
        return {
          result: await renderResult(params.template, runtimeInput),
          events: [renderLogEvent('Rendered template output') satisfies BridgeEvent],
        };
      case 'addTemplate.render':
        return {
          result: await renderAddTemplateResult(params.template, runtimeInput.context),
          events: [renderLogEvent('Rendered add template output') satisfies BridgeEvent],
        };
      default:
        return null;
    }
  },
};
