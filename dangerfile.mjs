import { danger, fail, message, warn } from 'danger';

const changed = [
  ...danger.git.created_files,
  ...danger.git.modified_files,
  ...danger.git.deleted_files,
];

const changedCodeFiles = changed.filter((file) => /\.(rs|ts|tsx|js|mjs|cjs)$/.test(file));
const changedTestFiles = changed.filter(
  (file) => /(^|\/)(test|tests)\//.test(file) || /\.test\.(ts|tsx|js|mjs|cjs|rs)$/.test(file),
);
const changedChangesets = changed.filter((file) => file.startsWith('.changeset/'));

if (changedCodeFiles.length > 0 && changedChangesets.length === 0) {
  warn('Code changed without a `.changeset/*` entry. Add one if this affects a published package.');
}

if (changedCodeFiles.length > 0 && changedTestFiles.length === 0) {
  message('No test files changed. Confirm this change does not require test updates.');
}

const body = danger.github.pr.body ?? '';
if (!/##\s*Testing/i.test(body)) {
  fail('PR description must include a `## Testing` section.');
}
if (!/##\s*Release Impact/i.test(body)) {
  warn('PR description should include a `## Release Impact` section.');
}
