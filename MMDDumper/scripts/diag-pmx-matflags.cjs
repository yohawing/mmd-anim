// Minimal PMX 2.0 material draw-flag reader (assumes BDEF1 vertices, additionalUV=0 - true
// for the generated self-shadow fixtures). Prints each material's name + draw flag bits.
const fs = require("fs");
const buf = fs.readFileSync(process.argv[2]);
let o = 0;
const u8 = () => buf.readUInt8(o++);
const i32 = () => { const v = buf.readInt32LE(o); o += 4; return v; };
const f32 = () => { const v = buf.readFloatLE(o); o += 4; return v; };
const sig = buf.toString("ascii", 0, 4); o = 4;
const version = f32();
const globalCount = u8();
const globals = [];
for (let i = 0; i < globalCount; i++) globals.push(u8());
const [encoding, addUv, vIdx, tIdx, mIdx, bIdx] = globals;
const dec = encoding === 0 ? "utf16le" : "utf8";
const text = () => { const len = i32(); const s = buf.toString(dec, o, o + len); o += len; return s; };
const idx = (size) => { o += size; }; // skip an index of given byte size
text(); text(); text(); text(); // names + comments

const vCount = i32();
for (let i = 0; i < vCount; i++) {
  o += 12 + 12 + 8; // pos, normal, uv
  o += addUv * 16;
  const wt = u8();
  if (wt === 0) o += bIdx; // BDEF1
  else if (wt === 1) { o += bIdx * 2 + 4; } // BDEF2
  else if (wt === 2) { o += bIdx * 4 + 16; } // BDEF4
  else if (wt === 3) { o += bIdx * 2 + 4 + 36; } // SDEF
  else if (wt === 4) { o += bIdx * 4 + 16; } // QDEF
  else throw new Error("unexpected weight type " + wt + " at vertex " + i);
  o += 4; // edge scale
}
const fCount = i32();
o += fCount * vIdx; // face vertex indices
const texCount = i32();
for (let i = 0; i < texCount; i++) text();

const matCount = i32();
const FLAG = { 0x01: "both-side", 0x02: "ground-shadow", 0x04: "cast(SSmap)", 0x08: "receive(SS)", 0x10: "edge" };
for (let i = 0; i < matCount; i++) {
  const nameL = text();
  text(); // name universal
  o += 16 + 12 + 4 + 12; // diffuse, specular, specPower, ambient
  const flag = u8();
  o += 16 + 4; // edge color, edge size
  idx(tIdx); idx(tIdx); // texture, sphere texture
  o += 1; // sphere mode
  const toonFlag = u8();
  if (toonFlag === 0) idx(tIdx); else o += 1;
  text(); // memo
  o += 4; // surface count
  const bits = Object.entries(FLAG).filter(([b]) => flag & Number(b)).map(([, n]) => n);
  console.log(`material[${i}] "${nameL}" flag=0x${flag.toString(16).padStart(2, "0")} -> [${bits.join(", ")}]`);
}
console.log(`(sig=${sig.trim()} version=${version} materials=${matCount})`);
