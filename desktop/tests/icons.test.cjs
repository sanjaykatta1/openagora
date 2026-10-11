// OpenAgora's app icon: the files electron-builder packages for each OS, made
// by build/make-icons.py, and the build settings that use them.
const assert = require('node:assert/strict');
const { test } = require('node:test');
const fs = require('node:fs');
const path = require('node:path');
const zlib = require('node:zlib');

const build = path.resolve(__dirname, '..', 'build');

function pngSize(png) {
  assert.equal(png.toString('hex', 0, 8), '89504e470d0a1a0a', 'a PNG');
  assert.equal(png.toString('latin1', 12, 16), 'IHDR');
  return [png.readUInt32BE(16), png.readUInt32BE(20)];
}

/** The alpha of a PNG's pixels: 8-bit RGBA, not interlaced, as make-icons.py
 *  writes them. */
function alpha(png) {
  const [width, height] = pngSize(png);
  assert.deepEqual([png[24], png[25], png[28]], [8, 6, 0], '8-bit RGBA, not interlaced');
  const data = [];
  for (let at = 8; at < png.length;) {
    const length = png.readUInt32BE(at);
    if (png.toString('latin1', at + 4, at + 8) === 'IDAT') data.push(png.subarray(at + 8, at + 8 + length));
    at += 12 + length;
  }
  const raw = zlib.inflateSync(Buffer.concat(data));
  const stride = width * 4;
  const out = Buffer.alloc(stride * height);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    for (let x = 0; x < stride; x++) {
      const left = x >= 4 ? out[y * stride + x - 4] : 0;
      const up = y > 0 ? out[(y - 1) * stride + x] : 0;
      const corner = x >= 4 && y > 0 ? out[(y - 1) * stride + x - 4] : 0;
      let predicted = 0;
      if (filter === 1) predicted = left;
      else if (filter === 2) predicted = up;
      else if (filter === 3) predicted = (left + up) >> 1;
      else if (filter === 4) {
        const p = left + up - corner;
        const [a, b, c] = [Math.abs(p - left), Math.abs(p - up), Math.abs(p - corner)];
        predicted = a <= b && a <= c ? left : b <= c ? up : corner;
      }
      out[y * stride + x] = (raw[y * (stride + 1) + 1 + x] + predicted) & 0xff;
    }
  }
  return (x, y) => out[(y * width + x) * 4 + 3];
}

function icnsEntries() {
  const data = fs.readFileSync(path.join(build, 'icon.icns'));
  assert.equal(data.toString('latin1', 0, 4), 'icns');
  assert.equal(data.readUInt32BE(4), data.length);
  const entries = {};
  for (let at = 8; at < data.length;) {
    const kind = data.toString('latin1', at, at + 4);
    const length = data.readUInt32BE(at + 4);
    assert.ok(length >= 8 && at + length <= data.length, kind);
    if (kind !== 'TOC ') entries[kind] = data.subarray(at + 8, at + length);
    at += length;
  }
  return entries;
}

test('the macOS icon has every size macOS reads, as PNGs', () => {
  const sizes = Object.fromEntries(Object.entries(icnsEntries()).map(([kind, png]) => [kind, pngSize(png)]));
  assert.deepEqual(sizes, {
    ic11: [32, 32], ic12: [64, 64], ic07: [128, 128], ic13: [256, 256],
    ic08: [256, 256], ic14: [512, 512], ic09: [512, 512], ic10: [1024, 1024],
  });
});

test('on macOS the picture sits in a rounded square with a clear margin', () => {
  const at = alpha(icnsEntries().ic10);
  // Apple's grid: an 824-pixel rounded square in the middle of 1024.
  assert.equal(at(0, 0), 0);
  assert.equal(at(60, 512), 0);
  assert.equal(at(140, 512), 255);
  assert.equal(at(512, 512), 255);
  assert.equal(at(512, 960), 0);
});

test('the Windows icon goes up to 256 pixels, uncompressed', () => {
  const data = fs.readFileSync(path.join(build, 'icon.ico'));
  assert.deepEqual([data.readUInt16LE(0), data.readUInt16LE(2)], [0, 1], 'an icon file');
  const sizes = [];
  for (let i = 0; i < data.readUInt16LE(4); i++) {
    const entry = 6 + 16 * i;
    const size = data[entry] || 256;
    const length = data.readUInt32LE(entry + 8);
    const offset = data.readUInt32LE(entry + 12);
    assert.equal(data[entry + 1] || 256, size);
    assert.equal(data.readUInt16LE(entry + 6), 32, 'bits per pixel');
    // A bitmap header (twice the height, for the mask), the pixels, the mask.
    assert.deepEqual([data.readUInt32LE(offset), data.readInt32LE(offset + 4), data.readInt32LE(offset + 8)], [40, size, size * 2]);
    assert.equal(length, 40 + size * size * 4 + Math.ceil(size / 32) * 4 * size);
    assert.ok(offset + length <= data.length);
    if (size === 256) {
      // Rows go bottom up: the top-left corner is clear, the middle isn't.
      const pixel = (x, y) => offset + 40 + ((size - 1 - y) * size + x) * 4;
      assert.equal(data[pixel(0, 0) + 3], 0);
      assert.equal(data[pixel(0, 128) + 3], 255);
      assert.equal(data[pixel(128, 128) + 3], 255);
    }
    sizes.push(size);
  }
  assert.deepEqual(sizes, [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]);
});

test('the Linux icons are named by their sizes and fill the square', () => {
  const sizes = [];
  for (const file of fs.readdirSync(path.join(build, 'icons'))) {
    const [, width, height] = /^(\d+)x(\d+)\.png$/.exec(file) ?? assert.fail(file);
    const png = fs.readFileSync(path.join(build, 'icons', file));
    assert.deepEqual(pngSize(png), [Number(width), Number(height)], file);
    sizes.push(Number(width));
  }
  assert.deepEqual(sizes.sort((a, b) => a - b), [16, 24, 32, 48, 64, 128, 256, 512]);
  const at = alpha(fs.readFileSync(path.join(build, 'icons', '512x512.png')));
  assert.equal(at(0, 0), 0, 'a rounded corner');
  assert.equal(at(0, 256), 255, 'no margin');
  assert.equal(at(256, 256), 255);
});

test('electron-builder packages them, and Linux windows get the 256-pixel one', () => {
  const { build: config } = require('../package.json');
  assert.equal(config.mac.icon, 'build/icon.icns');
  assert.equal(config.win.icon, 'build/icon.ico');
  assert.equal(config.linux.icon, 'build/icons');
  assert.deepEqual(config.linux.extraResources, [{ from: 'build/icons/256x256.png', to: 'icon.png' }]);
  // build/ itself never goes into the app.
  assert.ok(config.files.every((pattern) => !pattern.startsWith('build')));
});
