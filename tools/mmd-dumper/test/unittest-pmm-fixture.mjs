import iconv from "iconv-lite";

export function createUnittestOneBoneTemplate(options = {}) {
  const bytes = Buffer.alloc(0x600);
  Buffer.from("Polygon Movie maker 0002\0", "latin1").copy(bytes, 0);
  bytes.writeUInt16LE(1, 0x1ce);
  bytes.writeUInt32LE(30, 0x1d6);
  Buffer.from("14141414", "hex").copy(bytes, 0x1e2);
  if (options.modelPath) {
    const encoded = iconv.encode(options.modelPath, "cp932");
    encoded.copy(bytes, 0x260);
  }
  return bytes;
}
