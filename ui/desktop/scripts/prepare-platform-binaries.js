const fs = require('fs');
const path = require('path');
const { fetchLlamaServer } = require('./fetch-llama-server');

// Paths
const srcBinDir = path.join(__dirname, '..', 'src', 'bin');
const platformWinDir = path.join(__dirname, '..', 'src', 'platform', 'windows', 'bin');

// Platform-specific file patterns
const windowsFiles = ['*.exe', '*.dll', '*.cmd', 'biorouter-npm/**/*', 'git/**/*'];

const macosFiles = ['biorouterd', 'biorouter', 'jbang', 'npx', 'uvx', '*.db', '*.log', '.gitkeep'];

// Helper function to check if file matches patterns
function matchesPattern(filename, patterns) {
  return patterns.some((pattern) => {
    if (pattern.includes('**')) {
      // Handle directory patterns
      const basePattern = pattern.split('/**')[0];
      return filename.startsWith(basePattern);
    } else if (pattern.includes('*')) {
      // Handle wildcard patterns - be more precise with file extensions
      if (pattern.startsWith('*.')) {
        // For file extension patterns like *.exe, *.dll
        const extension = pattern.substring(2); // Remove "*."
        return filename.endsWith('.' + extension);
      } else {
        // For other wildcard patterns
        const regex = new RegExp('^' + pattern.replace(/\*/g, '.*') + '$');
        return regex.test(filename);
      }
    } else {
      // Exact match
      return filename === pattern;
    }
  });
}

// Helper function to clean directory of cross-platform files
function cleanBinDirectory(targetPlatform) {
  console.log(`Cleaning bin directory for ${targetPlatform} build...`);

  if (!fs.existsSync(srcBinDir)) {
    console.log('src/bin directory does not exist, skipping cleanup');
    return;
  }

  const files = fs.readdirSync(srcBinDir, { withFileTypes: true });

  files.forEach((file) => {
    const filePath = path.join(srcBinDir, file.name);

    if (targetPlatform === 'darwin' || targetPlatform === 'linux') {
      // For macOS/Linux, remove Windows-specific files
      if (matchesPattern(file.name, windowsFiles)) {
        console.log(`Removing Windows file: ${file.name}`);
        if (file.isDirectory()) {
          fs.rmSync(filePath, { recursive: true, force: true });
        } else {
          fs.unlinkSync(filePath);
        }
      }
    } else if (targetPlatform === 'win32') {
      // For Windows, remove macOS-specific files (keep only Windows files and common files)
      if (
        !matchesPattern(file.name, windowsFiles) &&
        !matchesPattern(file.name, ['*.db', '*.log', '.gitkeep'])
      ) {
        // Check if it's a macOS binary (executable without extension)
        if (file.isFile() && !path.extname(file.name) && file.name !== '.gitkeep') {
          try {
            // Check if file is executable (likely a macOS binary)
            const stats = fs.statSync(filePath);
            if (stats.mode & parseInt('111', 8)) {
              // Check if any execute bit is set
              console.log(`Removing macOS binary: ${file.name}`);
              fs.unlinkSync(filePath);
            }
          } catch (err) {
            console.warn(`Could not check file ${file.name}:`, err.message);
          }
        }
      }
    }
  });
}

// Helper function to copy platform-specific files
function copyPlatformFiles(targetPlatform) {
  if (targetPlatform === 'win32') {
    console.log('Copying Windows-specific files...');

    if (!fs.existsSync(platformWinDir)) {
      console.warn('Windows platform directory does not exist');
      return;
    }

    // Ensure src/bin exists
    if (!fs.existsSync(srcBinDir)) {
      fs.mkdirSync(srcBinDir, { recursive: true });
    }

    // Copy Windows-specific files
    const files = fs.readdirSync(platformWinDir, { withFileTypes: true });
    files.forEach((file) => {
      if (file.name === 'README.md' || file.name === '.gitignore') {
        return;
      }

      const srcPath = path.join(platformWinDir, file.name);
      const destPath = path.join(srcBinDir, file.name);

      if (file.isDirectory()) {
        fs.cpSync(srcPath, destPath, { recursive: true, force: true });
        console.log(`Copied directory: ${file.name}`);
      } else {
        fs.copyFileSync(srcPath, destPath);
        console.log(`Copied: ${file.name}`);
      }
    });
  }
}

// Validate that required platform binaries are present before packaging
function validateRequiredBinaries(targetPlatform) {
  // Both the server (biorouterd) AND the CLI (biorouter) must ship, so the app
  // can offer "install the Biorouter CLI" from its bundled binary.
  const required = {
    win32: ['biorouterd.exe', 'biorouter.exe', 'llamacpp/llama-server.exe'],
    darwin: ['biorouterd', 'biorouter', 'llamacpp/llama-server'],
    linux: ['biorouterd', 'biorouter', 'llamacpp/llama-server'],
  };

  const requiredForPlatform = required[targetPlatform];
  if (!requiredForPlatform) return;

  const missing = requiredForPlatform.filter((name) => {
    const fullPath = path.join(srcBinDir, name);
    return !fs.existsSync(fullPath);
  });

  if (missing.length > 0) {
    const platformLabel =
      targetPlatform === 'win32' ? 'Windows' : targetPlatform === 'darwin' ? 'macOS' : 'Linux';
    console.error(`\n❌ PACKAGING ERROR: Missing required ${platformLabel} binary/binaries:`);
    missing.forEach((name) => console.error(`   - ${path.join(srcBinDir, name)}`));
    console.error('\nBuild the backend first:');
    if (targetPlatform === 'win32') {
      console.error('  just release-windows   (cross-compile from macOS/Linux via Docker)');
      console.error('  just win-bld-rls       (native Windows build)');
    } else {
      console.error('  cargo build --release  (then: just copy-binary)');
    }
    console.error('');
    process.exit(1);
  }
}

// Main function
function preparePlatformBinaries() {
  const targetPlatform = process.env.ELECTRON_PLATFORM || process.platform;
  const targetArch = process.env.ELECTRON_ARCH || process.arch;

  console.log(`Preparing binaries for platform: ${targetPlatform} (${targetArch})`);

  // First copy platform-specific files if needed
  copyPlatformFiles(targetPlatform);

  // Then clean up cross-platform files
  cleanBinDirectory(targetPlatform);

  // Bundle the pinned llama-server sidecar for the Llama Server provider
  fetchLlamaServer(targetPlatform, targetArch);

  // Fail fast if the backend binary is absent — prevents silent broken packages
  validateRequiredBinaries(targetPlatform);

  console.log('Platform binary preparation complete');
}

// Run if called directly
if (require.main === module) {
  preparePlatformBinaries();
}

module.exports = { preparePlatformBinaries };
