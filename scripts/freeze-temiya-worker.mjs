// Freeze an already-measured worker before another Cargo build can replace it.
import { copyFileSync, constants, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
const [reportPath, source, executable, infoPath] = process.argv.slice(2);
if (!reportPath || !source || !executable || !infoPath) throw Error('usage: freeze-temiya-worker.mjs REPORT_JSON SOURCE_BINARY NEW_BINARY NEW_INFO_JSON');
if (existsSync(executable) || existsSync(infoPath)) throw Error('baseline output paths must be new');
const report = JSON.parse(readFileSync(reportPath));
const worker = report.workers?.[0];
const hash = createHash('sha256').update(readFileSync(source)).digest('hex');
if ((worker?.engine ?? worker?.name) !== 'Temiya' || !['AST interpreter', 'Auto Script', 'Auto Script+loop'].includes(worker.mode) ||
    worker.profile !== 'release' || !Array.isArray(worker.features) || worker.features.length !== 0 ||
    !/^[a-f0-9]{40}$/.test(worker.revision ?? '') || worker.executable_sha256 !== hash || typeof report.rustc !== 'string') {
  throw Error('source binary does not match a measured release Temiya worker');
}
copyFileSync(source, executable, constants.COPYFILE_EXCL);
writeFileSync(infoPath, JSON.stringify({ ...worker, name: 'Temiya', engine: 'Temiya', executable: resolve(executable),
  rustc: report.rustc, source_report: resolve(reportPath) }, null, 2) + '\n', { flag: 'wx' });
console.log(JSON.stringify({ executable: resolve(executable), info: resolve(infoPath), sha256: hash }));
