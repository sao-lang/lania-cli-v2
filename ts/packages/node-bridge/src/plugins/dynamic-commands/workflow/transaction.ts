import { mergeTransactionCompensationPlan } from '../transaction-engine.js';
import {
  readTransactionLabel,
  readTransactionOptions,
  readTransactionTarget,
} from './options.js';

// Transaction registration helpers.
// They let step handlers override rollback metadata without duplicating parsing logic.

export function resolveTransactionRegistration(
  options: unknown,
  fallback: {
    step: string;
    target: string;
    status: 'applied';
    rollback:
      | { kind: 'unsupported'; reason: string }
      | {
          kind: 'compensation';
          reason: string;
          plan: { commands?: Array<{ program: string; args: string[] }>; notes?: string[] };
        };
  },
): {
  step: string;
  target: string;
  status: 'applied';
  rollback:
    | { kind: 'unsupported'; reason: string }
    | { kind: 'none'; reason?: string }
    | {
        kind: 'compensation';
        reason: string;
        plan: { commands?: Array<{ program: string; args: string[] }>; notes?: string[] };
      };
} {
  const transaction = readTransactionOptions(options);
  const registration = transaction?.rollback;
  const step = readTransactionLabel(options, fallback.step);
  const target = readTransactionTarget(options, fallback.target);
  if (!registration || registration.kind === 'inherit') {
    return { ...fallback, step, target };
  }
  if (registration.kind === 'none') {
    return {
      step,
      target,
      status: fallback.status,
      rollback: { kind: 'none', reason: registration.reason },
    };
  }
  if (registration.kind === 'unsupported') {
    return {
      step,
      target,
      status: fallback.status,
      rollback: {
        kind: 'unsupported',
        reason: registration.reason,
      },
    };
  }
  return {
    step,
    target,
    status: fallback.status,
    rollback: {
      kind: 'compensation',
      reason: registration.reason,
      plan:
        mergeTransactionCompensationPlan(
          fallback.rollback.kind === 'compensation' ? fallback.rollback.plan : undefined,
          registration.compensation,
        ) ?? {},
    },
  };
}

