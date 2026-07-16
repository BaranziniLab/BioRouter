const { FusesPlugin } = require('@electron-forge/plugin-fuses');
const { FuseV1Options, FuseVersion } = require('@electron/fuses');
const { AutoUnpackNativesPlugin } = require('@electron-forge/plugin-auto-unpack-natives');
const { resolve } = require('path');

let cfg = {
  asar: true,
  extraResource: ['src/bin', 'src/images'],
  icon: 'src/images/icon',
  // macOS code signing and notarization
  // Activate by setting APPLE_ID and APPLE_APP_SPECIFIC_PASSWORD in the build environment.
  // Generate an app-specific password at https://appleid.apple.com/account/manage
  ...(process.env.APPLE_ID
    ? {
        osxSign: {
          identity: 'Developer ID Application: University of California at San Francisco (F3YYBXAFJ8)',
          hardenedRuntime: true,
          entitlements: 'entitlements.plist',
          'entitlements-inherit': 'entitlements.plist',
          'signature-flags': 'library',
        },
        // Notarization can be skipped (BIOROUTER_SKIP_NOTARIZE=1) for fast,
        // signed-but-not-notarized local/test builds — the signature alone is
        // enough for an in-place Squirrel.Mac update (identity match) and for
        // running locally without Gatekeeper quarantine. Release builds leave
        // it unset so the app is fully notarized + stapled.
        ...(process.env.BIOROUTER_SKIP_NOTARIZE === '1'
          ? {}
          : {
              osxNotarize: {
                tool: 'notarytool',
                appleId: process.env.APPLE_ID,
                appleIdPassword: process.env.APPLE_APP_SPECIFIC_PASSWORD,
                teamId: 'F3YYBXAFJ8',
              },
            }),
      }
    : {}),
  // Windows specific configuration
  win32: {
    icon: 'src/images/icon.ico',
    certificateFile: process.env.WINDOWS_CERTIFICATE_FILE,
    signingRole: process.env.WINDOW_SIGNING_ROLE,
    rfc3161TimeStampServer: 'http://timestamp.digicert.com',
    signWithParams: '/fd sha256 /tr http://timestamp.digicert.com /td sha256',
  },
  // Protocol registration
  protocols: [
    {
      name: 'BiorouterProtocol',
      schemes: ['biorouter'],
    },
  ],
  // macOS Info.plist extensions for drag-and-drop support
  extendInfo: {
    // Document types for drag-and-drop support onto dock icon
    CFBundleDocumentTypes: [
      {
        CFBundleTypeName: 'Folders',
        CFBundleTypeRole: 'Viewer',
        LSHandlerRank: 'Alternate',
        LSItemContentTypes: ['public.directory', 'public.folder'],
      },
      {
        CFBundleTypeName: 'Biorouter Extension Bundle',
        CFBundleTypeRole: 'Viewer',
        CFBundleTypeExtensions: ['brxt'],
        LSHandlerRank: 'Owner',
      },
    ],
  },
  // Windows file associations
  fileAssociations: [
    {
      ext: 'brxt',
      name: 'Biorouter Extension Bundle',
      description: 'Biorouter Extension Bundle',
      role: 'Viewer',
    },
  ],
};

module.exports = {
  packagerConfig: cfg,
  rebuildConfig: {},
  publishers: [
    {
      name: '@electron-forge/publisher-github',
      config: {
        repository: {
          owner: 'BaranziniLab',
          name: 'biorouter',
        },
        prerelease: false,
        draft: true,
      },
    },
  ],
  makers: [
    {
      name: '@electron-forge/maker-zip',
      platforms: ['darwin', 'win32', 'linux'],
      config: {
        arch: process.env.ELECTRON_ARCH === 'x64' ? ['x64'] : ['arm64'],
        options: {
          icon: 'src/images/icon.ico',
        },
      },
    },
    {
      name: '@electron-forge/maker-dmg',
      platforms: ['darwin'],
      config: {
        icon: './src/images/icon.icns',
        format: 'ULFO',
        overwrite: true,
      },
    },
    {
      name: '@electron-forge/maker-deb',
      config: {
        name: 'Biorouter',
        bin: 'Biorouter',
        maintainer: 'BaranziniLab',
        homepage: 'https://github.com/BaranziniLab/biorouter',
        categories: ['Development'],
        mimeType: ['application/x-biorouter-brxt'],
        desktopTemplate: './forge.deb.desktop',
        options: {
          icon: 'src/images/icon.png',
          prefix: '/opt',
          // Runtime deps of the bundled llama-server (Llama Server provider):
          // OpenSSL 3 and OpenMP. Implies Debian 12+ / Ubuntu 22.04+.
          depends: ['libssl3', 'libgomp1'],
        },
      },
    },
    {
      name: '@electron-forge/maker-rpm',
      config: {
        name: 'Biorouter',
        bin: 'Biorouter',
        maintainer: 'BaranziniLab',
        homepage: 'https://github.com/BaranziniLab/biorouter',
        categories: ['Development'],
        mimeType: ['application/x-biorouter-brxt'],
        desktopTemplate: './forge.rpm.desktop',
        options: {
          icon: 'src/images/icon.png',
          prefix: '/opt',
          // openssl-libs ships libssl.so.3 on EL9+/Fedora; libgomp for llama-server.
          requires: ['openssl-libs', 'libgomp'],
          fpm: ['--rpm-rpmbuild-define', '_build_id_links none'],
        },
      },
    },
    {
      name: '@electron-forge/maker-flatpak',
      config: {
        options: {
          categories: ['Development'],
          icon: 'src/images/icon.png',
          homepage: 'https://github.com/BaranziniLab/biorouter',
          runtimeVersion: '25.08',
          baseVersion: '25.08',
          bin: 'Biorouter',
          modules: [
            {
              name: 'libbz2-shim',
              buildsystem: 'simple',
              'build-commands': [
                // Create the lib directory in the app bundle
                'mkdir -p /app/lib',
                // Point to the actual library in the 25.08 runtime
                // We use a wildcard to handle multi-arch paths (x86_64-linux-gnu, etc)
                'ln -s $(find /usr/lib -name "libbz2.so.1" | head -n 1) /app/lib/libbz2.so.1.0'
              ]
            }
          ],
          finishArgs: [
            '--share=ipc',
            '--socket=x11',
            '--socket=wayland',
            '--device=dri',
            '--share=network',
            '--filesystem=home',
            '--talk-name=org.freedesktop.Notifications',
            '--socket=session-bus',
            '--socket=system-bus',
            // This ensures the app looks in our shim folder first
            '--env=LD_LIBRARY_PATH=/app/lib'
          ],
        },
      },
    },
  ],
  plugins: [
    {
      name: '@electron-forge/plugin-vite',
      config: {
        build: [
          {
            entry: 'src/main.ts',
            config: 'vite.main.config.mts',
          },
          {
            entry: 'src/preload.ts',
            config: 'vite.preload.config.mts',
          },
        ],
        renderer: [
          {
            name: 'main_window',
            config: 'vite.renderer.config.mts',
          },
        ],
      },
    },
    new AutoUnpackNativesPlugin({}),
    // Fuses are used to enable/disable various Electron functionality
    // at package time, before code signing the application
    new FusesPlugin({
      version: FuseVersion.V1,
      [FuseV1Options.RunAsNode]: false,
      [FuseV1Options.EnableCookieEncryption]: true,
      [FuseV1Options.EnableNodeOptionsEnvironmentVariable]: false,
      [FuseV1Options.EnableNodeCliInspectArguments]: false,
      [FuseV1Options.EnableEmbeddedAsarIntegrityValidation]: true,
      [FuseV1Options.OnlyLoadAppFromAsar]: true,
    }),
  ],
};
