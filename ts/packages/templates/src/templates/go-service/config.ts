export default () => ({
  outputTasks: ['write-files'],
  files: [
    { outputPath: 'go.mod', templatePath: 'ejs/go.mod.ejs' },
    { outputPath: 'main.go', templatePath: 'ejs/main.go.ejs' },
    { outputPath: 'lan.config.js', templatePath: 'ejs/lan.config.js.ejs' },
    { outputPath: '.gitignore', templatePath: 'ejs/.gitignore.ejs' },
    { outputPath: 'README.md', templatePath: 'ejs/README.md.ejs' },
  ],
});
