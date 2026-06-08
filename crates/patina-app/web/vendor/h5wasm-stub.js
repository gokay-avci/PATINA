export class Dataset {}
export class Group {}
export class File {
  constructor() {
    throw new Error('HDF5 trajectory parsing is not bundled in the PATINA desktop structure viewer.');
  }
}

export const ready = Promise.resolve({
  FS: {}
});
