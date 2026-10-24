export default () => ({
  outputTasks: ['write-files'],
  files: [
    { outputPath: 'pubspec.yaml', templatePath: 'ejs/pubspec.yaml.ejs' },
    { outputPath: 'bin/main.dart', templatePath: 'ejs/bin/main.dart.ejs' },
    { outputPath: 'lan.config.js', templatePath: 'ejs/lan.config.js.ejs' },
    { outputPath: '.gitignore', templatePath: 'ejs/.gitignore.ejs' },
    { outputPath: 'README.md', templatePath: 'ejs/README.md.ejs' },
  ],
});
