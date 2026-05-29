module.exports = {
  parserPreset: {
    parserOpts: {
      headerPattern: /^([^:]+):\s(.+)$/,
      headerCorrespondence: ['scope', 'subject'],
    },
  },
  plugins: [
    {
      rules: {
        'subject-lowercase': (parsed) => {
          const { subject } = parsed;
          if (!subject) return [false, 'Subject is required'];
          if (subject !== subject.toLowerCase())
            return [false, 'Subject must be lowercase'];
          return [true, ''];
        },
        'subject-no-trailing-period': (parsed) => {
          const { subject } = parsed;
          if (!subject) return [false, 'Subject is required'];
          if (subject.endsWith('.'))
            return [false, 'Subject must not end with a period'];
          return [true, ''];
        },
      },
    },
  ],
  rules: {
    'scope-empty': [2, 'never'],
    'header-max-length': [2, 'always', 72],
    'subject-lowercase': [2, 'always'],
    'subject-no-trailing-period': [2, 'always'],
  },
};
