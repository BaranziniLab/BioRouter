# BioRouter Documentation

This directory contains the built static documentation site for GitHub Pages deployment.

The documentation is built from the `documentation/` directory using Docusaurus.

## Local Development

To work on the documentation:

```bash
cd documentation
npm install
npm run start
```

## Building

```bash
cd documentation
npm run build
```

The built site will be in `documentation/build/` and will be automatically copied to `docs/` by GitHub Actions.

## Deployment

Documentation is automatically deployed to GitHub Pages when changes are pushed to the `main` branch.

View the documentation at: https://baranzinilab.github.io/BioRouter/

