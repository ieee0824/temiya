// Check the selected target and proposal metadata against immutable official data.
import fs from 'node:fs';
import {createHash} from 'node:crypto';
const read = name => JSON.parse(fs.readFileSync(new URL('../'+name, import.meta.url)));
const target = read('benchmarks/targets/spidermonkey.json');
const audit = read('benchmarks/targets/proposal-audit.json');
const support = read('spec-support.json');
if (audit.repository !== 'https://github.com/tc39/proposals' || audit.file !== 'finished-proposals.md' || audit.entries.length !== 4 || new Set(audit.entries.map(x => x.proposal)).size !== 4) throw Error('proposal audit coverage');
if (target.repository !== 'https://github.com/mozilla-firefox/firefox' || !/^[a-f0-9]{40}$/.test(target.revision)) throw Error('official source pin required');
if (target.standards['ECMA-262'].edition !== support.target.edition || target.standards['ECMA-262'].year !== support.target.year || audit.revision !== support.proposal_snapshot.revision) throw Error('target/snapshot drift');
const url = `https://raw.githubusercontent.com/tc39/proposals/${audit.revision}/${audit.file}`;
const response = await fetch(url);
if (!response.ok) throw Error(`official proposal source: ${response.status}`);
const body = Buffer.from(await response.arrayBuffer());
if (createHash('sha256').update(body).digest('hex') !== audit.file_sha256) throw Error('proposal source checksum mismatch');
const text = body.toString('utf8');
for (const entry of audit.entries) {
  const line = text.split('\n').find(line => line.startsWith('| ['+entry.proposal+']'));
  if (entry.stage !== 4 || !line || Number(line.match(/\|\s*(\d{4})\s*\|$/)?.[1]) !== entry.expected_publication_year) throw Error('proposal status/year mismatch: '+entry.proposal);
}
console.log('Official source pin, fixed language target and four proposal classifications verified');
