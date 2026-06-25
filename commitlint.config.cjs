module.exports = {
  extends: ['@commitlint/config-conventional'],
  rules: {
    // You can tighten these later (scope-enum, subject-case, etc.)
    'subject-empty': [2, 'never'],
    'type-empty': [2, 'never'],
  },
};
