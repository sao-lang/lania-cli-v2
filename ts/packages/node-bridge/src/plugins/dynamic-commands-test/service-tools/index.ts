// Split service-level helper coverage by domain so each test file stays
// reviewable while the public `registerTests` entry remains unchanged.
import { registerExecServiceTests } from './exec.js';
import { registerFilesystemServiceTests } from './filesystem.js';
import { registerPackageManagerServiceTests } from './package-manager.js';
import { registerPresentationServiceTests } from './presentation.js';
import { registerTaskServiceTests } from './tasks.js';

export function registerTests() {
  registerExecServiceTests();
  registerTaskServiceTests();
  registerPackageManagerServiceTests();
  registerFilesystemServiceTests();
  registerPresentationServiceTests();
}
