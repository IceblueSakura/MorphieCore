// Reserve synchronously before a request handler's first await. Invalid requests
// conservatively consume a local slot too; no refund/replay races.
export class ProbeSlots {
  #used = 0;
  readonly limit: number;
  constructor(limit: number) {
    if (!Number.isSafeInteger(limit) || limit < 1) throw new Error('invalid slot limit');
    this.limit = limit;
  }
  take() {
    if (this.#used >= this.limit) throw new Error('probe slot budget');
    return ++this.#used;
  }
  get used() { return this.#used; }
}
