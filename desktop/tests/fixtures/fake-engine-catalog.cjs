#!/usr/bin/env node
// Stands in for the engine's command line where OpenAgora's window runs it
// itself rather than through a page's request (which the UI tests fake):
// `catalog --json` lists an installed app that takes files by its own URL
// scheme, and one that isn't installed. Anything else fails.
const args = process.argv.slice(2);
if (args.includes('catalog') && args.includes('--json')) {
  const app = (id, installed) => ({
    id, name: id, installed,
    destination: { accepts: ['image/png'], deliver: 'url-scheme', adapter: null, folder: null, url: `${id}://open?file={file}` },
  });
  process.stdout.write(JSON.stringify([app('sketch', true), app('paint', false)]));
} else {
  process.stderr.write(`the stand-in engine doesn't do: ${args.join(' ')}\n`);
  process.exitCode = 1;
}
