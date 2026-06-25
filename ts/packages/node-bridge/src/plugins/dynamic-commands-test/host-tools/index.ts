// Split host-backed tool coverage by capability so each module stays small and
// the top-level `host-tools.ts` compatibility entry can remain stable.
import { registerToolAuditTests } from './audit.js';
import { registerHostFacadeTests } from './facade.js';
import { registerGroupedGitHelperTests } from './git.js';
import { registerInteractionToolTests } from './interaction.js';
import { registerToolsPolicyTests } from './policy.js';

export function registerTests() {
  registerHostFacadeTests();
  registerInteractionToolTests();
  registerGroupedGitHelperTests();
  registerToolsPolicyTests();
  registerToolAuditTests();
}
