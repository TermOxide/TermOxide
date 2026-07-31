import { RuleConfigSeverity } from "@commitlint/types";

const HEADER_MAX_LENGTH = 72; // Git/Github max length before truncation

// GitHub appends this to the subject on squash merge, e.g. "fix bug (#42)".
// Only that exact trailing suffix is exempt from the usual subject/length
// rules below — a hand-written commit ending in "(#42)" for any other
// reason still has to obey them normally.
const SQUASH_SUFFIX = /\s\(#\d+\)$/;
const stripSquashSuffix = (subject) => subject.replace(SQUASH_SUFFIX, '');

export default {
  parserPreset: {
    parserOpts: {
      headerPattern: /^([a-z0-9\-\/]+):\s(.+)$/,
      headerCorrespondence: ['scope', 'subject'],
    },
  },
  plugins: [
    {
      rules: {
        'scope-format': (parsed) => {
          const { scope } = parsed;
          if (!scope) return [false, 'Scope is required'];
          if (!/^[a-z0-9\-\/]+$/.test(scope))
            return [false, 'Scope must only contain lowercase letters, digits, hyphens, or slashes'];
          return [true, ''];
        },
        'subject-start-lowercase': (parsed) => {
          const { subject } = parsed;
          if (!subject) return [false, 'Subject is required'];
          if (!/^[a-z]/.test(subject))
            return [false, 'Subject must start with a lowercase letter'];
          return [true, ''];
        },
        'subject-end-alphanumeric': (parsed) => {
          const { subject } = parsed;
          if (!subject) return [false, 'Subject is required'];
          if (!/[a-zA-Z0-9]$/.test(stripSquashSuffix(subject)))
            return [false, 'Subject must end with a letter or a number'];
          return [true, ''];
        },
        'subject-no-double-spaces': (parsed) => {
          const { subject } = parsed;
          if (!subject) return [false, 'Subject is required'];
          if (/  +/.test(subject))
            return [false, 'Subject must not contain consecutive spaces'];
          return [true, ''];
        },
        'subject-imperative-mood': (parsed) => {
          const { subject } = parsed;
          if (!subject) return [false, 'Subject is required'];
          const firstWord = subject.split(' ')[0];
          const allowlist = ['pass', 'focus', 'bypass', 'alias', 'express'];
          if (allowlist.includes(firstWord)) return [true, ''];
          if (/(ed|ing|s)$/.test(firstWord))
            return [false, `The first word of the subject ('${firstWord}') must be in the imperative mood (e.g., 'remove' instead of 'removed' or 'removes')`];
          return [true, ''];
        },
        'header-max-length-squash-aware': (parsed) => {
          const { header } = parsed;
          // A squash merge can bundle several original commits' worth of
          // subject text; exempt it from the length check entirely rather
          // than trying to budget for how much a merge might add.
          if (SQUASH_SUFFIX.test(header)) return [true, ''];
          if (header.length > HEADER_MAX_LENGTH) {
            return [
              false,
              `header must not be longer than ${HEADER_MAX_LENGTH} characters, current length is ${header.length}`,
            ];
          }
          return [true, ''];
        },
      },
    },
  ],
  rules: {
    'header-max-length-squash-aware': [RuleConfigSeverity.Error, 'always'],
    'scope-empty': [RuleConfigSeverity.Error, 'never'],
    'scope-format': [RuleConfigSeverity.Error, 'always'],
    'subject-start-lowercase': [RuleConfigSeverity.Error, 'always'],
    'subject-end-alphanumeric': [RuleConfigSeverity.Error, 'always'],
    'subject-no-double-spaces': [RuleConfigSeverity.Error, 'always'],
    'subject-imperative-mood': [RuleConfigSeverity.Error, 'always'],
  },
};
