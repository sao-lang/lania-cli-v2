export default () => ({
  outputTasks: ['write-files'],
  files: [
    { outputPath: 'Cargo.toml', templatePath: 'ejs/Cargo.toml.ejs' },
    { outputPath: 'src/lib.rs', templatePath: 'ejs/src/lib.rs.ejs' },
    { outputPath: 'lan.config.js', templatePath: 'ejs/lan.config.js.ejs' },
    { outputPath: '.gitignore', templatePath: 'ejs/.gitignore.ejs' },
    { outputPath: 'README.md', templatePath: 'ejs/README.md.ejs' },
  ],
});
