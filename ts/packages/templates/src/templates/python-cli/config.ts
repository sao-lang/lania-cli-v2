export default () => ({
  outputTasks: ['write-files'],
  files: [
    { outputPath: 'pyproject.toml', templatePath: 'ejs/pyproject.toml.ejs' },
    { outputPath: 'src/main.py', templatePath: 'ejs/src/main.py.ejs' },
    { outputPath: 'lan.config.js', templatePath: 'ejs/lan.config.js.ejs' },
    { outputPath: '.gitignore', templatePath: 'ejs/.gitignore.ejs' },
    { outputPath: 'README.md', templatePath: 'ejs/README.md.ejs' },
  ],
});
