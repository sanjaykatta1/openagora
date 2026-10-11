// Does to a window what a window manager would: marks it managed (WM_STATE =
// NormalState) and raises it above the others. The UI tests run under a bare X
// server (Xvfb) with no window manager: Chromium lists only windows a window
// manager manages, and nothing keeps a window the user "clicked" on top.
// Speaks X11 itself, so the tests need no X tools.
const fs = require('node:fs');
const net = require('node:net');

const pad = (n) => (4 - (n % 4)) % 4;

/** The X server's cookie for this display, from XAUTHORITY. */
function cookie(display) {
  const file = process.env.XAUTHORITY;
  if (!file || !fs.existsSync(file)) return null;
  const data = fs.readFileSync(file);
  let at = 0;
  const field = () => {
    const length = data.readUInt16BE(at);
    const value = data.subarray(at + 2, at + 2 + length);
    at += 2 + length;
    return value;
  };
  while (at + 2 <= data.length) {
    at += 2; // address family
    field(); // address
    const number = field().toString();
    const name = field().toString();
    const value = field();
    if (name === 'MIT-MAGIC-COOKIE-1' && (number === display || number === '')) return value;
  }
  return null;
}

function connect(display) {
  const socket = net.connect(`/tmp/.X11-unix/X${display}`);
  let buffered = Buffer.alloc(0);
  let waiting = null;
  const settle = () => {
    if (waiting && buffered.length >= waiting.n) {
      const chunk = buffered.subarray(0, waiting.n);
      buffered = buffered.subarray(waiting.n);
      const { resolve } = waiting;
      waiting = null;
      resolve(chunk);
    }
  };
  socket.on('data', (chunk) => { buffered = Buffer.concat([buffered, chunk]); settle(); });
  socket.on('error', (error) => { if (waiting) waiting.reject(error); });
  const read = (n) => new Promise((resolve, reject) => { waiting = { n, resolve, reject }; settle(); });
  return { socket, read };
}

async function reply(read, what) {
  const packet = await read(32);
  if (packet[0] !== 1) throw new Error(`X11 ${what} failed (error ${packet[1]})`);
  const extra = packet.readUInt32LE(4);
  if (extra) await read(extra * 4);
  return packet;
}

/** Opens a connection to the X server and runs `work(socket, read)`. */
async function withX(work) {
  const display = (process.env.DISPLAY ?? '').replace(/^[^:]*:/, '').split('.')[0];
  if (!display) throw new Error('no X display');
  const { socket, read } = connect(display);
  try {
    const auth = cookie(display);
    const name = auth ? Buffer.from('MIT-MAGIC-COOKIE-1') : Buffer.alloc(0);
    const data = auth ?? Buffer.alloc(0);
    const setup = Buffer.alloc(12);
    setup.write('l', 0); // little-endian
    setup.writeUInt16LE(11, 2);
    setup.writeUInt16LE(name.length, 6);
    setup.writeUInt16LE(data.length, 8);
    socket.write(Buffer.concat([setup, name, Buffer.alloc(pad(name.length)), data, Buffer.alloc(pad(data.length))]));
    const head = await read(8);
    await read(head.readUInt16LE(6) * 4);
    if (head[0] !== 1) throw new Error('the X server refused the connection');
    await work(socket, read);
    // A round trip: an error in what was sent would come back first.
    const focus = Buffer.alloc(4);
    focus.writeUInt8(43, 0); // GetInputFocus
    focus.writeUInt16LE(1, 2);
    socket.write(focus);
    await reply(read, 'request');
  } finally {
    socket.destroy();
  }
}

/** Gives the X11 window `id` WM_STATE = NormalState. */
async function markNormal(id) {
  await withX(async (socket, read) => {
    const atom = Buffer.from('WM_STATE');
    const intern = Buffer.alloc(8);
    intern.writeUInt8(16, 0); // InternAtom
    intern.writeUInt16LE(2 + (atom.length + pad(atom.length)) / 4, 2);
    intern.writeUInt16LE(atom.length, 4);
    socket.write(Buffer.concat([intern, atom, Buffer.alloc(pad(atom.length))]));
    const wmState = (await reply(read, 'InternAtom')).readUInt32LE(8);

    const change = Buffer.alloc(32);
    change.writeUInt8(18, 0); // ChangeProperty, Replace
    change.writeUInt16LE(8, 2);
    change.writeUInt32LE(id, 4);
    change.writeUInt32LE(wmState, 8);
    change.writeUInt32LE(wmState, 12);
    change.writeUInt8(32, 16);
    change.writeUInt32LE(2, 20);
    change.writeUInt32LE(1, 24); // NormalState
    change.writeUInt32LE(0, 28); // no icon window
    socket.write(change);
  });
}

/** Raises the X11 window `id` above every other window. */
async function raise(id) {
  await withX(async (socket) => {
    const configure = Buffer.alloc(16);
    configure.writeUInt8(12, 0); // ConfigureWindow
    configure.writeUInt16LE(4, 2);
    configure.writeUInt32LE(id, 4);
    configure.writeUInt16LE(0x40, 8); // stack-mode
    configure.writeUInt32LE(0, 12); // Above
    socket.write(configure);
  });
}

module.exports = { markNormal, raise };
