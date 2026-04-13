const fs = require('fs');
const path = require('path');
const https = require('https');
const { execSync } = require('child_process');

const VERSION = require('../package.json').version;
const REPO = 'Shin-sibainu/ccmux';

function getPlatformBinary() {
  const platform = process.platform;
  const arch = process.arch;

  if (platform === 'win32' && arch === 'x64') return 'ccmux-windows-x64.exe';
  if (platform === 'darwin' && arch === 'arm64') return 'ccmux-macos-arm64';
  if (platform === 'darwin' && arch === 'x64') return 'ccmux-macos-x64';
  if (platform === 'linux' && arch === 'x64') return 'ccmux-linux-x64';

  console.error(`Unsupported platform: ${platform}-${arch}`);
  process.exit(1);
}

function download(url, dest) {
  return new Promise((resolve, reject) => {
    const follow = (url) => {
      https.get(url, { headers: { 'User-Agent': 'ccmux-installer' } }, (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          follow(res.headers.location);
          return;
        }
        if (res.statusCode !== 200) {
          reject(new Error(`Download failed: HTTP ${res.statusCode}`));
          return;
        }
        const file = fs.createWriteStream(dest);
        res.pipe(file);
        file.on('finish', () => {
          file.close();
          resolve();
        });
      }).on('error', reject);
    };
    follow(url);
  });
}

async function main() {
  const binaryName = getPlatformBinary();
  const url = `https://github.com/${REPO}/releases/download/v${VERSION}/${binaryName}`;
  const binDir = path.join(__dirname, '..', 'bin');
  const isWindows = process.platform === 'win32';
  const dest = path.join(binDir, isWindows ? 'ccmux.exe' : 'ccmux');

  console.log(`Downloading ccmux v${VERSION} for ${process.platform}-${process.arch}...`);

  try {
    await download(url, dest);

    if (!isWindows) {
      fs.chmodSync(dest, 0o755);
    }

    console.log('ccmux installed successfully!');
  } catch (err) {
    console.error(`Failed to download ccmux: ${err.message}`);
    console.error(`URL: ${url}`);
    console.error('You can download manually from: https://github.com/Shin-sibainu/ccmux/releases');
    process.exit(1);
  }
}

main();
