import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { brotliCompressSync, constants, gzipSync } from "node:zlib";

const root = new URL("../out", import.meta.url).pathname;
const exts = /\.(js|css|html|txt|svg|json|map|webmanifest|ico)$/;
let n = 0;
function walk(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p);
    else if (exts.test(name) && statSync(p).size > 1024) {
      const data = readFileSync(p);
      writeFileSync(`${p}.br`, brotliCompressSync(data, { params: { [constants.BROTLI_PARAM_QUALITY]: 11, [constants.BROTLI_PARAM_SIZE_HINT]: data.length } }));
      writeFileSync(`${p}.gz`, gzipSync(data, { level: 9 }));
      n++;
    }
  }
}
walk(root);
console.log(`compressed ${n} files`);
