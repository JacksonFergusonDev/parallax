// Stands in for the engine's other formats and its zip library. Books reach the
// reader as EPUBs through Rust, so nothing calls these.
const unsupported = () => {
  throw new Error("The reader only opens EPUBs");
};

export const makeComicBook = unsupported;
export const makeFB2 = unsupported;
export const makePDF = unsupported;
export const isMOBI = unsupported;
export const MOBI = unsupported;
export const configure = unsupported;
export const ZipReader = unsupported;
export const BlobReader = unsupported;
export const TextWriter = unsupported;
export const BlobWriter = unsupported;
export const unzlibSync = unsupported;
